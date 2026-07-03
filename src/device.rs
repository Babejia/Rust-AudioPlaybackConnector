//! Bluetooth A2DP Sink device enumeration and connection management.

use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::*;
use windows::Devices::Enumeration::*;
use windows::Foundation::*;
use windows::Media::Audio::*;

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceState { Available, Connecting, Connected, Failed(String) }
#[derive(Clone, Debug)]
pub struct DeviceEntry { pub id: String, pub name: String, pub state: DeviceState }

static CONNECTIONS: Mutex<Option<ConnectionManager>> = Mutex::new(None);

struct ConnectionInfo {
    _device_info: DeviceInformation,
    connection: AudioPlaybackConnection,
    _state_changed_token: EventRegistrationToken,
}

struct ConnectionManager {
    connections: HashMap<String, ConnectionInfo>,
    on_device_state_changed: Box<dyn Fn(String, DeviceState) + Send>,
}

impl ConnectionManager {
    fn new<F: Fn(String, DeviceState) + Send + 'static>(cb: F) -> Self {
        ConnectionManager { connections: HashMap::new(), on_device_state_changed: Box::new(cb) }
    }
    fn add(&mut self, id: &str, info: ConnectionInfo) { self.connections.insert(id.to_string(), info); }
    fn remove(&mut self, id: &str) -> Option<ConnectionInfo> { self.connections.remove(id) }
    fn is_connected(&self, id: &str) -> bool { self.connections.contains_key(id) }
    fn connected_ids(&self) -> Vec<String> { self.connections.keys().cloned().collect() }
    fn close_all(&mut self) { for (_, info) in self.connections.drain() { let _ = info.connection.Close(); } }
}

pub fn init<F: Fn(String, DeviceState) + Send + 'static>(cb: F) {
    *CONNECTIONS.lock().unwrap() = Some(ConnectionManager::new(cb));
}

pub fn is_supported() -> bool {
    windows::Foundation::Metadata::ApiInformation::IsTypePresent(
        &HSTRING::from("Windows.Media.Audio.AudioPlaybackConnection")
    ).unwrap_or(false)
}

pub fn enumerate_a2dp_devices() -> std::result::Result<Vec<DeviceEntry>, Box<dyn std::error::Error>> {
    let sel = AudioPlaybackConnection::GetDeviceSelector()?;
    let op = DeviceInformation::FindAllAsyncAqsFilter(&sel)?;
    let results = op.get()?;
    let count = results.Size()?;
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count {
        let device = results.GetAt(i)?;
        let id = device.Id()?.to_string_lossy();
        let name = device.Name()?.to_string_lossy();
        let st = if is_device_connected(&id) { DeviceState::Connected } else { DeviceState::Available };
        entries.push(DeviceEntry { id, name, state: st });
    }
    Ok(entries)
}

/// Connect to a Bluetooth A2DP Sink device.
///
/// This function:
/// 1. Gets the DeviceInformation for the given device ID
/// 2. Calls AudioPlaybackConnection::TryCreateFromId to try to open a connection
/// 3. Registers a StateChanged handler to track disconnections
/// 4. Calls StartAsync then OpenAsync to establish the audio stream
/// 5. On success, stores the connection in the global connection registry
pub fn connect_to_device(device_id: &str) -> std::result::Result<(), String> {
    // Step 1: Get the DeviceInformation object for this device ID.
    // We use CreateFromIdAsync to get full device info, then use its deviceId
    // for the AudioPlaybackConnection. This matches the original C++ behavior
    // where the DevicePicker provides the DeviceInformation.
    let id_h = HSTRING::from(device_id);
    let device_op = DeviceInformation::CreateFromIdAsync(&id_h)
        .map_err(|e| format!("CreateFromIdAsync failed: {}", e))?;
    let device = device_op.get()
        .map_err(|e| format!("CreateFromIdAsync get failed: {}", e))?;

    let device_id_h = device.Id()
        .map_err(|e| format!("device.Id() failed: {}", e))?;
    let device_id_str = device_id_h.to_string_lossy();

    // Step 2: Try to create an AudioPlaybackConnection for this device.
    let connection = AudioPlaybackConnection::TryCreateFromId(&device_id_h)
        .map_err(|e| format!("TryCreateFromId failed: {}", e))?;

    // Step 3: Register StateChanged handler BEFORE starting the connection.
    // This ensures we catch state changes, including spontaneous disconnections.
    let cb_remove_id = device_id_str.clone();
    let cb_notify_id = device_id_str.clone();

    let handler = TypedEventHandler::<AudioPlaybackConnection, IInspectable>::new(
        move |sender: &Option<AudioPlaybackConnection>, _: &Option<IInspectable>| {
            if let Some(s) = sender {
                if let Ok(state) = s.State() {
                    if state == AudioPlaybackConnectionState::Closed {
                        // Connection closed — clean up
                        let device_id_for_close = s.DeviceId()
                            .map(|h| h.to_string_lossy())
                            .unwrap_or_default();
                        let mut g = CONNECTIONS.lock().unwrap();
                        if let Some(ref mut mgr) = *g {
                            let id_to_remove = if !device_id_for_close.is_empty() {
                                device_id_for_close.as_str()
                            } else {
                                cb_remove_id.as_str()
                            };
                            mgr.remove(id_to_remove);
                            (mgr.on_device_state_changed)(cb_notify_id.clone(), DeviceState::Available);
                        }
                    }
                }
            }
            Ok(())
        },
    );

    let token = connection.StateChanged(&handler)
        .map_err(|e| format!("StateChanged failed: {}", e))?;

    // Step 4: StartAsync kicks off the Bluetooth connection process.
    connection.StartAsync()
        .map_err(|e| format!("StartAsync failed: {}", e))?
        .get()
        .map_err(|e| format!("StartAsync get failed: {}", e))?;

    // Step 5: OpenAsync waits for the connection to establish and creates
    // the audio endpoint which Windows uses to route audio to the device.
    let result = connection.OpenAsync()
        .map_err(|e| format!("OpenAsync failed: {}", e))?
        .get()
        .map_err(|e| format!("OpenAsync get failed: {}", e))?;

    let status = result.Status()
        .map_err(|e| format!("result.Status() failed: {}", e))?;

    match status {
        AudioPlaybackConnectionOpenResultStatus::Success => {
            // Connection established and audio endpoint created!
            // The system audio should now be routable to this device.
            let info = ConnectionInfo {
                _device_info: device,
                connection,
                _state_changed_token: token,
            };
            let mut g = CONNECTIONS.lock().unwrap();
            if let Some(ref mut mgr) = *g {
                mgr.add(&device_id_str, info);
            }
            Ok(())
        }
        AudioPlaybackConnectionOpenResultStatus::RequestTimedOut => {
            Err("The request timed out".into())
        }
        AudioPlaybackConnectionOpenResultStatus::DeniedBySystem => {
            Err("The operation was denied by the system".into())
        }
        AudioPlaybackConnectionOpenResultStatus::UnknownFailure => {
            // Try to get extended error info for debugging
            let ext = result.ExtendedError()
                .unwrap_or(HRESULT(0));
            Err(format!("Unknown failure (HRESULT: 0x{:08X})", ext.0))
        }
        _ => Err("Unexpected result status".into()),
    }
}

pub fn disconnect_device(device_id: &str) {
    let mut g = CONNECTIONS.lock().unwrap();
    if let Some(ref mut mgr) = *g {
        if let Some(info) = mgr.remove(device_id) {
            let _ = info.connection.Close();
        }
    }
}

pub fn is_device_connected(device_id: &str) -> bool {
    match *CONNECTIONS.lock().unwrap() { Some(ref m) => m.is_connected(device_id), None => false }
}

pub fn get_connected_device_ids() -> Vec<String> {
    match *CONNECTIONS.lock().unwrap() { Some(ref m) => m.connected_ids(), None => Vec::new() }
}

pub fn close_all_connections() {
    let mut g = CONNECTIONS.lock().unwrap();
    if let Some(ref mut m) = *g { m.close_all(); }
}

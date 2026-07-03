//! SVG icon rendering via Direct2D.

use std::ptr;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn svg_to_hicon(svg: &str, width: i32, height: i32, color: D2D1_COLOR_F) -> HICON {
    unsafe {
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap();
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 0.0, dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let dc_rt: ID2D1DCRenderTarget = factory.CreateDCRenderTarget(&props).unwrap();
        let hdc = CreateCompatibleDC(None);

        let bmi_h = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width, biHeight: height,
            biPlanes: 1, biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let bmi = BITMAPINFO { bmiHeader: bmi_h, bmiColors: [RGBQUAD::default(); 1] };
        let mut bits: *mut std::ffi::c_void = ptr::null_mut();
        let hbitmap = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();

        let bmi_mh = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width, biHeight: height,
            biPlanes: 1, biBitCount: 1,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let bmi_m = BITMAPINFO { bmiHeader: bmi_mh, bmiColors: [RGBQUAD::default(); 1] };
        let mut bits_m: *mut std::ffi::c_void = ptr::null_mut();
        let hbitmap_mask = CreateDIBSection(hdc, &bmi_m, DIB_RGB_COLORS, &mut bits_m, None, 0).unwrap();

        let _old = SelectObject(hdc, hbitmap);
        let r = RECT { left: 0, top: 0, right: width, bottom: height };
        dc_rt.BindDC(hdc, &r).unwrap();

        let stream = SHCreateMemStream(Some(svg.as_bytes())).unwrap();
        let dc5: ID2D1DeviceContext5 = dc_rt.cast().unwrap();
        dc5.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);

        let sz = D2D_SIZE_F { width: width as f32, height: height as f32 };
        let doc: ID2D1SvgDocument = dc5.CreateSvgDocument(&stream, sz).unwrap();
        let root: ID2D1SvgElement = doc.GetRoot().unwrap();

        let cs = format!("#{:02X}{:02X}{:02X}",
            (color.r * 255.0) as u8, (color.g * 255.0) as u8, (color.b * 255.0) as u8);
        let cw: Vec<u16> = cs.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = root.SetAttributeValue3(w!("fill"), D2D1_SVG_ATTRIBUTE_STRING_TYPE_SVG, PCWSTR(cw.as_ptr()));

        dc5.BeginDraw();
        dc5.DrawSvgDocument(&doc);
        let _ = dc5.EndDraw(None, None);

        let ii = ICONINFO { fIcon: true.into(), hbmMask: hbitmap_mask, hbmColor: hbitmap, xHotspot: 0, yHotspot: 0 };
        let hicon = CreateIconIndirect(&ii).unwrap();
        let _ = DeleteObject(hbitmap);
        let _ = DeleteObject(hbitmap_mask);
        let _ = DeleteDC(hdc);
        hicon
    }
}

pub fn d2d_color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

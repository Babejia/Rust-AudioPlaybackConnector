fn main() {
    embed_manifest::embed_manifest_file("app.manifest").expect("failed to embed manifest");
    embed_resource::compile("app.rc", std::iter::empty::<&str>());
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=desk_ico/futaba.ico");
}

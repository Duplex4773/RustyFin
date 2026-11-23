fn main() {
    println!("cargo:rerun-if-changed=native/rustyfin.rc");
    println!("cargo:rerun-if-changed=assets/brand/waterline.ico");
    embed_resource::compile("native/rustyfin.rc", embed_resource::NONE)
        .manifest_required()
        .expect("embedding the RustyFin application icon");
}

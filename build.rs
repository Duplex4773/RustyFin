use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=native/rustyfin.rc");
    println!("cargo:rerun-if-changed=assets/brand/waterline.ico");
    embed_resource::compile("native/rustyfin.rc", embed_resource::NONE)
        .manifest_required()
        .expect("embedding the RustyFin application icon");

    println!("cargo:rerun-if-changed=assets/shaders");
    // Keep third-party shaders replaceable beside the executable, including
    // their notices. OUT_DIR is under <profile>/build/<package>/out.
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let shaders = out
        .ancestors()
        .nth(3)
        .expect("Cargo profile directory")
        .join("shaders");
    fs::create_dir_all(&shaders).expect("creating the shader directory");
    for asset in fs::read_dir("assets/shaders").expect("reading bundled shaders") {
        let asset = asset.expect("reading a bundled shader");
        fs::copy(asset.path(), shaders.join(asset.file_name()))
            .expect("installing a bundled shader");
    }
}

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=shaders/overlay.vert");
    println!("cargo:rerun-if-changed=shaders/overlay.frag");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    for (src, dst) in [
        ("shaders/overlay.vert", "overlay.vert.spv"),
        ("shaders/overlay.frag", "overlay.frag.spv"),
    ] {
        let out_path = out_dir.join(dst);
        let status = Command::new("glslc")
            .args([src, "-O", "-o"])
            .arg(&out_path)
            .status()
            .expect("failed to run glslc");
        assert!(status.success(), "glslc failed for {src}");
    }
}

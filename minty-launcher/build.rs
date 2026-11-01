use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let java_source = manifest_dir.join("java").join("MintyBootstrap.java");
    println!("cargo:rerun-if-changed={}", java_source.display());

    let output = Command::new("javac")
        .arg("--release")
        .arg("8")
        .arg("-encoding")
        .arg("UTF-8")
        .arg("-d")
        .arg(&out_dir)
        .arg(&java_source)
        .output()
        .unwrap_or_else(|e| panic!("Не удалось запустить javac. Установите JDK 21 и добавьте javac в PATH: {e}"));

    if !output.status.success() {
        panic!(
            "Не удалось собрать Java bootstrap:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("cargo:rustc-env=MINTY_BOOTSTRAP_CLASS={}", out_dir.join("MintyBootstrap.class").display());

    let icon_png = manifest_dir.join("assets").join("minty-launcher-icon.png");
    println!("cargo:rerun-if-changed={}", icon_png.display());
    let icon_bytes = std::fs::read(&icon_png)
        .unwrap_or_else(|e| panic!("Не удалось прочитать иконку {}: {e}", icon_png.display()));
    if icon_bytes.len() < 24 || !icon_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        panic!("assets/minty-launcher-icon.png is not a valid PNG image");
    }
    let width = u32::from_be_bytes(icon_bytes[16..20].try_into().expect("PNG width"));
    let height = u32::from_be_bytes(icon_bytes[20..24].try_into().expect("PNG height"));
    if width != 256 || height != 256 {
        panic!("Minty icon must be 256x256, got {width}x{height}");
    }

    #[cfg(windows)]
    {
        // Build the ICO from the canonical PNG so the icon is embedded in
        // the PE executable. No external image/ICO file is needed at runtime.
        let icon_ico = out_dir.join("minty-launcher.ico");
        let image_size = u32::try_from(icon_bytes.len()).expect("PNG icon too large");
        let mut ico = Vec::with_capacity(22 + icon_bytes.len());
        ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
        ico.extend_from_slice(&[0, 0, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&image_size.to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&icon_bytes);
        std::fs::write(&icon_ico, ico).expect("Не удалось создать ICO для Windows resource");

        let mut resource = winres::WindowsResource::new();
        resource.set("ProductName", "Minty Launcher");
        resource.set("FileDescription", "Minty Launcher for Minecraft");
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        resource.set("OriginalFilename", "minty-launcher.exe");
        resource.set_icon(icon_ico.to_str().expect("Windows icon path must be valid UTF-8"));
        resource.compile().expect("Не удалось встроить иконку и метаданные в minty-launcher.exe");
    }
}

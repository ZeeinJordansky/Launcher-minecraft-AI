use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    println!("cargo:rerun-if-changed=assets/minty-launcher-icon.png");
    let icon_png = manifest_dir.join("assets").join("minty-launcher-icon.png");
    let icon_bytes = std::fs::read(&icon_png)
        .unwrap_or_else(|error| panic!("Не удалось прочитать иконку {}: {error}", icon_png.display()));
    if icon_bytes.len() < 24 || !icon_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        panic!("assets/minty-launcher-icon.png is not a valid PNG image");
    }
    let width = u32::from_be_bytes(icon_bytes[16..20].try_into().expect("PNG width"));
    let height = u32::from_be_bytes(icon_bytes[20..24].try_into().expect("PNG height"));
    if width != 256 || height != 256 { panic!("Minty icon must be 256x256, got {width}x{height}"); }
    #[cfg(windows)]
    {
        let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
        let ico_path = out.join("minty-launcher.ico");
        let size = u32::try_from(icon_bytes.len()).expect("PNG icon too large");
        let mut ico = Vec::with_capacity(22 + icon_bytes.len());
        ico.extend_from_slice(&[0,0,1,0,1,0]);
        ico.extend_from_slice(&[0,0,0,0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&size.to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&icon_bytes);
        std::fs::write(&ico_path, ico).expect("Не удалось создать ICO");
        let mut resource = winres::WindowsResource::new();
        resource.set("ProductName", "Minty Launcher");
        resource.set("FileDescription", "Minty Launcher for Minecraft");
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        resource.set("OriginalFilename", "minty-launcher.exe");
        resource.set_icon(ico_path.to_str().expect("ICO path must be UTF-8"));
        resource.compile().expect("Не удалось встроить иконку в EXE");
    }
}

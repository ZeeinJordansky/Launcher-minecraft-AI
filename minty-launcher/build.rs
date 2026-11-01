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

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set("ProductName", "Minty Launcher");
        resource.set("FileDescription", "Minty Launcher for Minecraft");
        resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        let _ = resource.compile();
    }
}

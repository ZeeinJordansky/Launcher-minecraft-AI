use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, process::Command, time::Duration};
use zip::ZipArchive;

#[derive(Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}
#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}
#[derive(Deserialize)]
#[derive(Clone)]
struct AdoptiumPackage {
    link: String,
    checksum: Option<String>,
}

pub fn ensure_java(required_major: u32, custom_path: &str, runtime_root: &Path) -> Result<PathBuf> {
    if !custom_path.trim().is_empty() {
        let custom = PathBuf::from(custom_path.trim());
        if custom.is_file() {
            if java_major(&custom).map(|m| m >= required_major).unwrap_or(false) {
                return Ok(custom);
            }
            bail!("Указанный Java не подходит для этой версии Minecraft. Нужна Java {required_major} или новее.");
        }
        bail!("Файл Java не найден: {}", custom.display());
    }

    if let Ok(path) = find_on_path() {
        if java_major(&path).map(|m| is_compatible(m, required_major)).unwrap_or(false) {
            return Ok(path);
        }
    }

    let install_dir = runtime_root.join(format!("temurin-{required_major}"));
    if let Some(java) = find_java_executable(&install_dir) {
        if java_major(&java).map(|m| is_compatible(m, required_major)).unwrap_or(false) {
            return Ok(java);
        }
    }

    fs::create_dir_all(runtime_root)?;
    install_temurin(required_major, &install_dir)?;
    let java = find_java_executable(&install_dir).ok_or_else(|| anyhow!("Java Runtime скачан, но java.exe не найден"))?;
    if !java_major(&java).map(|m| is_compatible(m, required_major)).unwrap_or(false) {
        bail!("Установленная Java не соответствует требуемой версии {required_major}");
    }
    Ok(java)
}

fn is_compatible(found: u32, required: u32) -> bool {
    if required <= 16 { found == required } else { found >= required }
}

fn find_on_path() -> Result<PathBuf> {
    #[cfg(windows)]
    let candidate = PathBuf::from("java.exe");
    #[cfg(not(windows))]
    let candidate = PathBuf::from("java");
    let output = Command::new(&candidate).arg("-version").output()?;
    if !output.status.success() {
        bail!("Java не найдена в PATH");
    }
    // Resolve the executable from PATH using where/which for a stable path.
    #[cfg(windows)]
    let resolver = Command::new("where").arg("java.exe").output()?;
    #[cfg(not(windows))]
    let resolver = Command::new("which").arg("java").output()?;
    let first = String::from_utf8_lossy(&resolver.stdout).lines().next().unwrap_or("java").trim().to_owned();
    Ok(PathBuf::from(first))
}

fn version_probe_executable(java: &Path) -> PathBuf {
    let is_javaw = java.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("javaw.exe"))
        .unwrap_or(false);

    if is_javaw {
        let console_java = java.with_file_name("java.exe");
        if console_java.is_file() {
            return console_java;
        }
    }
    java.to_path_buf()
}

fn java_major(java: &Path) -> Result<u32> {
    let probe = version_probe_executable(java);
    let output = Command::new(&probe).arg("-version").output()
        .with_context(|| format!("Не удалось запустить Java для проверки версии: {}", probe.display()))?;
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
    let marker = text.split_once("version ").map(|(_, tail)| tail)
        .or_else(|| text.split_once("openjdk ").map(|(_, tail)| tail))
        .ok_or_else(|| anyhow!("Не удалось определить версию Java"))?;
    let version = marker.trim_start_matches(&['\"', '\''][..]);
    let first = version.split(['.', '-', '+', '\"', '\'']).next().unwrap_or_default();
    let number: u32 = first.parse().context("Некорректный номер версии Java")?;
    Ok(if number == 1 {
        version.split('.').nth(1).unwrap_or("8").parse().unwrap_or(8)
    } else {
        number
    })
}

fn install_temurin(major: u32, destination: &Path) -> Result<()> {
    let client = Client::builder().user_agent(concat!("MintyLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180)).build()?;
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&heap_size=normal&image_type=jre&os=windows&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client.get(url).send()?.error_for_status()?.json()
        .context("Adoptium не вернул подходящий Java Runtime")?;
    let package = assets.first().ok_or_else(|| anyhow!("Для Java {major} не найден Temurin JRE под Windows x64"))?.binary.package.clone();
    let archive_path = destination.with_extension("zip");
    fs::create_dir_all(destination.parent().unwrap_or_else(|| Path::new(".")))?;

    let response = client.get(&package.link).send()?.error_for_status()?;
    let mut archive_bytes = Vec::new();
    response.take(1024 * 1024 * 1024).read_to_end(&mut archive_bytes)?;
    if let Some(expected) = package.checksum.as_deref() {
        let actual = hex::encode(Sha256::digest(&archive_bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            bail!("Контрольная сумма Java Runtime не совпала; архив отклонён.");
        }
    }
    fs::write(&archive_path, &archive_bytes)?;
    extract_zip_safely(&archive_path, destination)?;
    let _ = fs::remove_file(archive_path);
    Ok(())
}

fn extract_zip_safely(archive_path: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::create_dir_all(destination)?;
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).context("Некорректный архив Java Runtime")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() { fs::create_dir_all(parent)?; }
            let mut out = fs::File::create(&output)?;
            std::io::copy(&mut entry, &mut out)?;
            out.flush()?;
        }
    }
    Ok(())
}

fn find_java_executable(root: &Path) -> Option<PathBuf> {
    if !root.exists() { return None; }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("javaw.exe")).unwrap_or(false) {
                return Some(path);
            }
        }
    }
    None
}


#[cfg(test)]
mod tests {
    use super::version_probe_executable;
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("minty-java-probe-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn javaw_version_probe_uses_sibling_java_exe() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let javaw = dir.join("javaw.exe");
        let java = dir.join("java.exe");
        fs::write(&javaw, b"stub").unwrap();
        fs::write(&java, b"stub").unwrap();

        assert_eq!(version_probe_executable(&javaw), java);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn javaw_version_probe_falls_back_if_console_java_is_missing() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let javaw = dir.join("javaw.exe");
        fs::write(&javaw, b"stub").unwrap();

        assert_eq!(version_probe_executable(&javaw), javaw);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regular_java_executable_is_used_for_version_probe() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let java = dir.join("java.exe");
        fs::write(&java, b"stub").unwrap();

        assert_eq!(version_probe_executable(&java), java);
        fs::remove_dir_all(dir).unwrap();
    }
}

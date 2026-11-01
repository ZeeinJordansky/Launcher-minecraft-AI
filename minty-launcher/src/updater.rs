use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::{fs, process::Command, time::Duration};

const RELEASE_API: &str = "https://api.github.com/repos/ZeeinJordansky/Launcher-minecraft-AI/releases/latest";

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub release_url: String,
    pub asset_url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub fn check_latest() -> Result<Option<UpdateInfo>> {
    let response = client()?.get(RELEASE_API).send()
        .context("Не удалось подключиться к GitHub Releases")?;
    if response.status().as_u16() == 404 {
        bail!("GitHub вернул HTTP 404 для API релизов. Репозиторий Minty Launcher сейчас приватный, поэтому без авторизации лаунчер не может автоматически проверять обновления. Ручная страница релизов открывается после входа в GitHub.");
    }
    let release: Release = response.error_for_status()
        .context("GitHub не вернул последний релиз")?
        .json().context("GitHub вернул некорректные сведения о релизе")?;
    if !is_newer(&release.tag_name, env!("CARGO_PKG_VERSION")) { return Ok(None); }
    let asset = release.assets.iter()
        .find(|asset| asset.name.eq_ignore_ascii_case("minty-launcher.exe"))
        .ok_or_else(|| anyhow!("В новом релизе отсутствует minty-launcher.exe"))?;
    Ok(Some(UpdateInfo {
        version: release.tag_name,
        release_url: release.html_url,
        asset_url: asset.browser_download_url.clone(),
        notes: release.body,
    }))
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |value: &str| -> Vec<u64> {
        value.trim().trim_start_matches(['v', 'V']).split('.')
            .map(|part| part.chars().take_while(|ch| ch.is_ascii_digit())
                .collect::<String>().parse().unwrap_or(0))
            .collect()
    };
    let left = parse(remote);
    let right = parse(current);
    for index in 0..left.len().max(right.len()) {
        let a = *left.get(index).unwrap_or(&0);
        let b = *right.get(index).unwrap_or(&0);
        if a != b { return a > b; }
    }
    false
}

pub fn download_and_schedule(info: &UpdateInfo) -> Result<()> {
    let response = client()?.get(&info.asset_url).send()
        .context("Не удалось загрузить обновление")?
        .error_for_status().context("GitHub не разрешил скачивание EXE")?;
    let bytes = response.bytes().context("Не удалось прочитать скачанный EXE")?;
    if bytes.len() < 1024 * 1024 || !bytes.starts_with(b"MZ") {
        bail!("Скачанный файл не похож на Windows EXE. Обновление отменено.");
    }
    let current_exe = std::env::current_exe().context("Не удалось определить путь установленного лаунчера")?;
    let suffix = std::process::id();
    let temp = std::env::temp_dir();
    let staged = temp.join(format!("minty-launcher-update-{suffix}.exe"));
    let helper = temp.join(format!("minty-launcher-updater-{suffix}.exe"));
    fs::write(&staged, &bytes).context("Не удалось сохранить обновление во временную папку")?;
    fs::copy(&current_exe, &helper)
        .context("Не удалось подготовить помощник обновления. Попробуй скачать EXE вручную.")?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new(&helper)
            .arg("--apply-update")
            .arg(&staged)
            .arg(&current_exe)
            .arg(std::process::id().to_string())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .context("Не удалось запустить помощник обновления")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = fs::remove_file(&staged);
        let _ = fs::remove_file(&helper);
        bail!("Автоматическое обновление поддерживается только в Windows.")
    }
}

#[cfg(windows)]
pub fn run_helper(args: &[String]) -> Result<()> {
    if args.len() < 5 || args.get(1).map(String::as_str) != Some("--apply-update") {
        bail!("Некорректные аргументы помощника обновления.");
    }
    let staged = std::path::PathBuf::from(&args[2]);
    let target = std::path::PathBuf::from(&args[3]);
    let parent_pid: u32 = args[4].parse().context("Некорректный PID лаунчера")?;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut std::ffi::c_void;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x00100000;
    const INFINITE: u32 = 0xFFFF_FFFF;
    let process = unsafe { OpenProcess(SYNCHRONIZE, 0, parent_pid) };
    if process.is_null() { bail!("Не удалось дождаться закрытия Minty Launcher."); }
    unsafe {
        WaitForSingleObject(process, INFINITE);
        CloseHandle(process);
    }

    let mut last_error = None;
    let mut copied = false;
    for _ in 0..40 {
        match fs::copy(&staged, &target) {
            Ok(_) => { copied = true; break; }
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(Duration::from_millis(350));
            }
        }
    }
    if !copied {
        return Err(last_error.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "copy failed")))
            .context("Не удалось заменить EXE. Запусти скачанный файл вручную.");
    }
    let _ = fs::remove_file(staged);
    Command::new(&target).spawn().context("Обновление установлено, но лаунчер не удалось открыть повторно")?;
    Ok(())
}

#[cfg(not(windows))]
pub fn run_helper(_args: &[String]) -> Result<()> {
    bail!("Помощник обновления доступен только в Windows.")
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("MintyLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(300))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::is_newer;
    #[test]
    fn compares_release_versions() {
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
    }
}
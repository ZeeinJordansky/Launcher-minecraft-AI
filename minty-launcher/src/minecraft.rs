use crate::{auth::Account, config::AppConfig, runtime, system_info};
use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::Sender;
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, fs, io::{Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc}, time::Duration};
use zip::ZipArchive;

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: Option<String>,
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    latest: LatestVersions,
    versions: Vec<VersionInfo>,
}
#[derive(Debug, Deserialize)]
struct LatestVersions {
    release: String,
    #[allow(dead_code)]
    snapshot: String,
}

#[derive(Debug, Clone)]
pub enum LaunchEvent {
    Status(String),
    Installed(String),
    Finished(String),
    Cancelled,
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
    path: Option<String>,
}

pub fn fetch_versions() -> Result<(Vec<VersionInfo>, String)> {
    let client = http_client()?;
    let manifest: VersionManifest = client.get(VERSION_MANIFEST).send()?.error_for_status()?.json()
        .context("Не удалось получить список версий Minecraft")?;
    let latest = manifest.latest.release;
    let versions = manifest.versions;
    Ok((versions, latest))
}

pub fn install_version(config: AppConfig, events: Sender<LaunchEvent>) -> Result<()> {
    let account = Account {
        name: if config.offline_username.trim().is_empty() { "Player".to_owned() } else { config.offline_username.clone() },
        uuid: String::new(), xuid: String::new(), access_token: String::new(),
    };
    let cancel = AtomicBool::new(false);
    match launch_inner(&config, &account, &events, false, &cancel) {
        Ok((_code, profile_id)) => {
            events.send(LaunchEvent::Installed(profile_id)).ok();
            Ok(())
        }
        Err(error) => {
            events.send(LaunchEvent::Failed(format!("{error:#}"))).ok();
            Err(error)
        }
    }
}

pub fn launch(config: AppConfig, account: Account, events: Sender<LaunchEvent>, cancel: Arc<AtomicBool>) -> Result<()> {
    let result = launch_inner(&config, &account, &events, true, &cancel);
    match result {
        Ok((_code, _profile_id)) => {
            events.send(LaunchEvent::Finished("Minecraft запущен. Лаунчер можно оставить открытым.".to_owned())).ok();
            Ok(())
        }
        Err(_error) if cancel.load(Ordering::Relaxed) => {
            events.send(LaunchEvent::Cancelled).ok();
            Ok(())
        }
        Err(error) => { events.send(LaunchEvent::Failed(format!("{error:#}"))).ok(); Err(error) }
    }
}

fn launch_inner(config: &AppConfig, account: &Account, events: &Sender<LaunchEvent>, launch_game: bool, cancel: &AtomicBool) -> Result<(i32, String)> {
    check_cancelled(cancel)?;
    let client = http_client()?;
    let version = if config.selected_version == "latest-release" {
        status(events, "Проверяю список версий…");
        fetch_versions()?.1
    } else { config.selected_version.clone() };
    validate_version_id(&version)?;

    check_cancelled(cancel)?;
    let hardware = system_info::detect();
    let low_end_profile = config.weak_pc_mode || (config.auto_graphics_profile && hardware.low_end);
    let game_dir = config.game_dir.clone();
    let versions_dir = game_dir.join("versions").join(&version);
    let libraries_dir = game_dir.join("libraries");
    let assets_dir = game_dir.join("assets");
    fs::create_dir_all(&game_dir)?;
    for dir in [&versions_dir, &libraries_dir, &assets_dir] { fs::create_dir_all(dir)?; }

    check_cancelled(cancel)?;
    status(events, &format!("Получаю метаданные Minecraft {version}…"));
    let manifest: VersionManifest = client.get(VERSION_MANIFEST).send()?.error_for_status()?.json()?;
    let version_info = manifest.versions.iter().find(|v| v.id == version)
        .ok_or_else(|| anyhow!("Версия {version} отсутствует в официальном манифесте Mojang"))?;
    let metadata_path = versions_dir.join(format!("{version}.json"));
    download_verified(&client, &version_info.url, &metadata_path, version_info.sha1.as_deref())?;
    let base_metadata: Value = serde_json::from_slice(&fs::read(&metadata_path)?)?;
    check_cancelled(cancel)?;
    let profile_id = ensure_loader_profile(config, &client, &version, &game_dir, events)?;
    check_cancelled(cancel)?;

    // Mutable data belongs to an optional isolated profile; common downloads stay shared.
    let instance_dir = crate::config::prepare_instance_dir(config)?;
    apply_weak_pc_profile(&instance_dir, low_end_profile)?;
    let metadata = if profile_id == version {
        base_metadata.clone()
    } else {
        let profile_path = game_dir.join("versions").join(&profile_id).join(format!("{profile_id}.json"));
        let profile: Value = serde_json::from_slice(&fs::read(&profile_path)?)
            .with_context(|| format!("Профиль {} повреждён", profile_id))?;
        merge_loader_profile(&base_metadata, &profile)?
    };

    let natives_dir = game_dir.join("natives").join(&profile_id);
    fs::create_dir_all(&natives_dir)?;
    let client_download = metadata["downloads"]["client"].clone();
    if client_download.is_null() { bail!("Для версии {version} нет официального client.jar"); }
    let client_url = client_download["url"].as_str().ok_or_else(|| anyhow!("В метаданных отсутствует URL client.jar"))?;
    let client_sha = client_download["sha1"].as_str();
    let client_jar = versions_dir.join(format!("{version}.jar"));
    status(events, "Проверяю и загружаю Minecraft client.jar…");
    download_verified(&client, client_url, &client_jar, client_sha)?;

    status(events, "Загружаю официальные библиотеки…");
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_archives: Vec<PathBuf> = Vec::new();
    if let Some(libraries) = metadata["libraries"].as_array() {
        for library in libraries {
            check_cancelled(cancel)?;
            if !library_allowed(library) { continue; }
            if let Some((path, url, sha1)) = resolve_library_artifact(library, &libraries_dir)? {
                download_verified(&client, &url, &path, sha1.as_deref())?;
                classpath.push(path);
            }
            if let Some(native_info) = native_download(library) {
                let path = libraries_dir.join(native_info.path.as_deref().unwrap_or("native.jar"));
                download_verified(&client, &native_info.url, &path, native_info.sha1.as_deref())?;
                native_archives.push(path);
            }
        }
    }
    classpath.push(client_jar.clone());

    status(events, "Распаковываю системные библиотеки Minecraft…");
    if natives_dir.exists() { fs::remove_dir_all(&natives_dir)?; }
    fs::create_dir_all(&natives_dir)?;
    check_cancelled(cancel)?;
    for archive in native_archives {
        check_cancelled(cancel)?;
        extract_natives(&archive, &natives_dir)?;
    }

    let asset_index_id = metadata["assetIndex"]["id"].as_str()
        .or_else(|| metadata["assets"].as_str()).unwrap_or("legacy").to_owned();
    if let Some(asset_index) = metadata.get("assetIndex").filter(|value| value.is_object()) {
        let asset_index_url = asset_index["url"].as_str()
            .ok_or_else(|| anyhow!("В метаданных отсутствует assetIndex.url"))?;
        let index_path = assets_dir.join("indexes").join(format!("{asset_index_id}.json"));
        status(events, "Загружаю индекс ресурсов…");
        download_verified(&client, asset_index_url, &index_path, asset_index["sha1"].as_str())?;
        let index: Value = serde_json::from_slice(&fs::read(&index_path)?)?;
        let objects = index["objects"].as_object().ok_or_else(|| anyhow!("Индекс ресурсов повреждён"))?;
    let asset_entries: Vec<(String, String)> = objects.values().filter_map(|object| {
        let hash = object["hash"].as_str()?;
        if hash.len() < 2 || !hash.is_ascii() { return None; }
        Some((hash.to_owned(), format!("https://resources.download.minecraft.net/{}/{}", &hash[..2], hash)))
    }).collect();
    let total_assets = asset_entries.len();
    let completed = Arc::new(AtomicUsize::new(0));
    status(events, &format!("Проверяю ресурсы Minecraft ({total_assets} файлов)…"));
    let asset_root = assets_dir.join("objects");
    let errors: Vec<String> = asset_entries.par_iter().filter_map(|(hash, url)| {
        if cancel.load(Ordering::Relaxed) { return None; }
        let path = asset_root.join(&hash[..2]).join(hash);
        let result = download_verified(&client, url, &path, Some(hash));
        completed.fetch_add(1, Ordering::Relaxed);
        result.err().map(|e| format!("{hash}: {e}"))
    }).collect();
        if cancel.load(Ordering::Relaxed) { check_cancelled(cancel)?; }
        if !errors.is_empty() {
            bail!("Не удалось загрузить {} ресурсов. Первый сбой: {}", errors.len(), errors[0]);
        }
    }
    check_cancelled(cancel)?;

    // Old Alpha/Beta versions may have no modern asset index or log config.
    let log_config_path = if let Some(file) = metadata["logging"]["client"]["file"].as_object() {
        let id = file.get("id").and_then(Value::as_str).unwrap_or("client-logging.xml");
        let url = file.get("url").and_then(Value::as_str).ok_or_else(|| anyhow!("Не указан URL logging config"))?;
        let path = assets_dir.join("log_configs").join(id);
        download_verified(&client, url, &path, file.get("sha1").and_then(Value::as_str))?;
        path
    } else {
        PathBuf::new()
    };

    if !launch_game {
        status(events, &format!("Файлы Minecraft {profile_id} установлены."));
        return Ok((0, profile_id));
    }

    check_cancelled(cancel)?;
    let java_major = metadata["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    status(events, &format!("Проверяю Java {java_major}…"));
    let runtime_root = crate::config::data_dir()?.join("runtimes");
    let java = runtime::ensure_java(java_major, &config.java_path, &runtime_root)?;

    let main_class = metadata["mainClass"].as_str().ok_or_else(|| anyhow!("Не указана mainClass в метаданных Minecraft"))?;
    let mut variables = HashMap::new();
    variables.insert("auth_player_name", account.name.as_str());
    variables.insert("version_name", profile_id.as_str());
    let game_dir_text = instance_dir.to_string_lossy().to_string();
    let assets_text = assets_dir.to_string_lossy().to_string();
    let natives_text = natives_dir.to_string_lossy().to_string();
    let classpath_text = std::env::join_paths(&classpath)?.to_string_lossy().to_string();
    let library_text = libraries_dir.to_string_lossy().to_string();
    let uuid = if account.uuid.trim().is_empty() { offline_uuid(&account.name) } else { account.uuid.clone() };
    variables.insert("auth_uuid", uuid.as_str());
    variables.insert("auth_access_token", if account.access_token.is_empty() { "0" } else { account.access_token.as_str() });
    variables.insert("user_type", if account.access_token.is_empty() { "legacy" } else { "msa" });
    variables.insert("auth_xuid", account.xuid.as_str());
    variables.insert("clientid", config.microsoft_client_id.as_str());
    variables.insert("version_type", metadata["type"].as_str().unwrap_or("release"));
    variables.insert("assets_index_name", asset_index_id.as_str());
    let log_config_text = log_config_path.to_string_lossy().to_string();
    variables.insert("path", log_config_text.as_str());
    variables.insert("log_config", log_config_text.as_str());
    variables.insert("classpath_separator", if cfg!(windows) { ";" } else { ":" });
    variables.insert("game_directory", game_dir_text.as_str());
    variables.insert("assets_root", assets_text.as_str());
    variables.insert("natives_directory", natives_text.as_str());
    variables.insert("classpath", classpath_text.as_str());
    variables.insert("library_directory", library_text.as_str());
    variables.insert("user_properties", "{}");
    variables.insert("resolution_width", "854");
    variables.insert("resolution_height", "480");
    variables.insert("auth_session", if account.access_token.is_empty() { "0" } else { account.access_token.as_str() });
    variables.insert("launcher_name", "Minty Launcher");
    variables.insert("launcher_version", env!("CARGO_PKG_VERSION"));

    let mut jvm_args = Vec::new();
    let safe_manual_max = hardware.total_memory_gb.saturating_sub(2).min(16).max(2) as u8;
    let max_memory_gb = if config.auto_memory {
        hardware.recommended_memory_gb
    } else {
        config.max_memory_gb.clamp(2, safe_manual_max)
    };
    jvm_args.push(if low_end_profile { "-Xms512M".to_owned() } else { "-Xms1G".to_owned() });
    jvm_args.push(format!("-Xmx{max_memory_gb}G"));
    if let Some(jvm) = metadata["arguments"]["jvm"].as_array() {
        jvm_args.extend(parse_arguments(jvm, &variables)?);
    } else {
        jvm_args.push(format!("-Djava.library.path={natives_text}"));
        jvm_args.push("-cp".to_owned());
        jvm_args.push(classpath_text.clone());
    }

    let mut game_args = Vec::new();
    if let Some(game) = metadata["arguments"]["game"].as_array() {
        game_args.extend(parse_arguments(game, &variables)?);
    } else if let Some(legacy) = metadata["minecraftArguments"].as_str() {
        // Legacy Minecraft argument strings are space-delimited; current release versions use the structured format.
        for part in legacy.split_whitespace() { game_args.push(expand(part, &variables)?); }
    } else {
        bail!("Формат аргументов этой версии Minecraft не поддерживается.");
    }

    match config.gc_algorithm.as_str() {
        "ZGC" => jvm_args.push("-XX:+UseZGC".to_owned()),
        "Shenandoah" => jvm_args.push("-XX:+UseShenandoahGC".to_owned()),
        "G1GC" | "Авто" | "" => jvm_args.push("-XX:+UseG1GC".to_owned()),
        _ => jvm_args.push("-XX:+UseG1GC".to_owned()),
    }
    if low_end_profile && matches!(config.gc_algorithm.as_str(), "Авто" | "G1GC" | "") {
        jvm_args.push("-XX:MaxGCPauseMillis=100".to_owned());
        jvm_args.push("-XX:+ParallelRefProcEnabled".to_owned());
    }
    if config.launch_fullscreen {
        game_args.push("--fullscreen".to_owned());
    }

    status(events, &format!("Запускаю Minecraft {profile_id}…"));
    let java = runtime::resolve_launch_executable(&java);
    if !java.is_file() {
        bail!("Исполняемый файл Java не найден: {}. Укажи java.exe в настройках или переустанови Java Runtime.", java.display());
    }
    ensure_game_directory_writable(&instance_dir)?;
    let log_path = crate::config::data_dir()?.join("logs").join("minecraft.log");
    if let Some(parent) = log_path.parent() { fs::create_dir_all(parent)?; }
    let log_file = fs::OpenOptions::new().create(true).append(true).open(&log_path)
        .with_context(|| format!("Не удалось открыть журнал игры: {}", log_path.display()))?;
    let stdout = log_file.try_clone()?;
    check_cancelled(cancel)?;
    let mut command = Command::new(&java);
    command.args(&jvm_args).arg(main_class).args(&game_args)
        .current_dir(&instance_dir).stdin(Stdio::null())
        .stdout(Stdio::from(stdout)).stderr(Stdio::from(log_file));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = command.spawn().with_context(|| {
        let os_code = std::io::Error::last_os_error().raw_os_error();
        if os_code == Some(5) {
            format!(
                "Windows вернул «Отказано в доступе» (Win32 error 5) при запуске Java.\nJava: {}\nПапка игры: {}\n\nMinty запускает игру через javaw.exe без отдельного консольного окна. Проверь права доступа к этим путям и исключения антивируса.",
                java.display(), game_dir.display()
            )
        } else {
            format!("Не удалось запустить Minecraft через {} (Win32 error {:?})", java.display(), os_code)
        }
    })?;
    status(events, &format!("Minecraft запущен (PID {}). Журнал: {}", child.id(), log_path.display()));
    drop(child);
    Ok((0, profile_id))
}

/// Applies conservative in-game graphics settings and keeps a one-time backup of options.txt.
/// When the user disables the profile, the exact original file is restored on next launch.
fn apply_weak_pc_profile(game_dir: &Path, enabled: bool) -> Result<()> {
    let options = game_dir.join("options.txt");
    let backup = game_dir.join("options.txt.minty-backup");
    let created_marker = game_dir.join("options.txt.minty-created");

    if !enabled {
        if backup.is_file() {
            fs::copy(&backup, &options).context("Не удалось восстановить исходные настройки Minecraft")?;
            fs::remove_file(&backup)?;
        } else if created_marker.is_file() {
            // There was no options file before the first optimized launch. Remove the generated
            // profile file so Minecraft can recreate its defaults the next time it starts.
            if options.is_file() {
                fs::remove_file(&options)?;
            }
        }
        if created_marker.exists() {
            fs::remove_file(&created_marker)?;
        }
        return Ok(());
    }

    if !options.is_file() {
        fs::write(&options, "")?;
        fs::write(&created_marker, "Minty created this file for the weak-PC profile.\n")?;
    } else if !backup.exists() && !created_marker.exists() {
        fs::copy(&options, &backup).context("Не удалось сохранить резервную копию options.txt")?;
    }

    let contents = fs::read_to_string(&options).context("Не удалось прочитать options.txt")?;
    let optimizations = [
        // Current names (newer Minecraft) and legacy names are both kept for version compatibility.
        ("graphicsMode", "0"),
        ("graphics", "fast"),
        ("renderDistance", "6"),
        ("simulationDistance", "4"),
        ("particles", "1"),
        ("cloudStatus", "0"),
        ("clouds", "false"),
        ("entityShadows", "false"),
        ("entityDistanceScaling", "0.5"),
        ("biomeBlendRadius", "0"),
        ("mipmapLevels", "0"),
        ("maxFps", "60"),
        ("ao", "0"),
    ];
    let mut values = std::collections::BTreeMap::<String, String>::new();
    for line in contents.lines() {
        if let Some((key, value)) = line.split_once(':') {
            values.insert(key.to_owned(), value.to_owned());
        }
    }
    for (key, value) in optimizations {
        values.insert(key.to_owned(), value.to_owned());
    }
    let mut output = String::new();
    for (key, value) in values {
        output.push_str(&key);
        output.push(':');
        output.push_str(&value);
        output.push('\n');
    }
    let temp = options.with_extension("txt.minty-tmp");
    fs::write(&temp, output)?;
    if options.exists() {
        fs::remove_file(&options)?;
    }
    fs::rename(temp, options)?;
    Ok(())
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Запуск Minecraft отменён пользователем.");
    }
    Ok(())
}

fn ensure_game_directory_writable(game_dir: &Path) -> Result<()> {
    fs::create_dir_all(game_dir)
        .with_context(|| format!("Не удалось создать папку игры: {}", game_dir.display()))?;
    let probe = game_dir.join(format!(".minty-access-check-{}.tmp", std::process::id()));
    fs::write(&probe, b"minty")
        .with_context(|| format!("Нет прав на запись в папку Minecraft: {}", game_dir.display()))?;
    fs::remove_file(&probe)
        .with_context(|| format!("Не удалось удалить временный файл проверки из {}", game_dir.display()))?;
    Ok(())
}

fn status(events: &Sender<LaunchEvent>, text: &str) {
    events.send(LaunchEvent::Status(text.to_owned())).ok();
}

fn http_client() -> Result<Client> {
    Ok(Client::builder().user_agent(concat!("MintyLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180)).build()?)
}

fn validate_version_id(id: &str) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        bail!("Некорректный идентификатор версии Minecraft");
    }
    Ok(())
}

fn download_verified(client: &Client, url: &str, path: &Path, expected_sha1: Option<&str>) -> Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    if path.is_file() {
        if expected_sha1.map(|hash| sha1_file(path).map(|actual| actual.eq_ignore_ascii_case(hash)).unwrap_or(false)).unwrap_or(true) {
            return Ok(());
        }
    }
    let response = client.get(url).send()?.error_for_status().with_context(|| format!("Не удалось скачать {url}"))?;
    let bytes = response.bytes()?;
    if let Some(expected) = expected_sha1 {
        let actual = hex::encode(Sha1::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) { bail!("SHA-1 не совпал для {url}"); }
    }
    let temp = path.with_extension("minty-download");
    fs::write(&temp, &bytes)?;
    if path.exists() { fs::remove_file(path)?; }
    fs::rename(temp, path)?;
    Ok(())
}

fn sha1_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 { break; }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn ensure_loader_profile(
    config: &AppConfig, client: &Client, game_version: &str,
    game_dir: &Path, events: &Sender<LaunchEvent>,
) -> Result<String> {
    let loader = match config.selected_loader.to_ascii_lowercase().as_str() {
        "vanilla" => return Ok(game_version.to_owned()),
        "fabric" => "Fabric", "quilt" => "Quilt", "forge" => "Forge", "neoforge" => "NeoForge",
        other => bail!("Неизвестный загрузчик: {other}"),
    };
    if !config.selected_profile_id.is_empty() {
        let path = game_dir.join("versions").join(&config.selected_profile_id)
            .join(format!("{}.json", config.selected_profile_id));
        if let Ok(bytes) = fs::read(path) {
            if let Ok(profile) = serde_json::from_slice::<Value>(&bytes) {
                if profile["inheritsFrom"].as_str() == Some(game_version)
                    && profile["id"].as_str().unwrap_or("").to_ascii_lowercase().contains(&loader.to_ascii_lowercase()) {
                    return Ok(config.selected_profile_id.clone());
                }
            }
        }
    }
    if loader == "Fabric" || loader == "Quilt" {
        status(events, &format!("Получаю официальный профиль {loader}…"));
        let api = if loader == "Fabric" {
            "https://meta.fabricmc.net/v2/versions/loader"
        } else {
            "https://meta.quiltmc.org/v3/versions/loader"
        };
        let url = format!("{api}/{}", url_encode_component(game_version));
        let versions: Value = client.get(url).send()?.error_for_status()?.json()
            .with_context(|| format!("Не удалось получить версии {loader}"))?;
        let entries = versions.as_array().ok_or_else(|| anyhow!("API {loader} вернул неверный ответ"))?;
        let loader_version = if config.selected_loader_version.trim().is_empty() {
            entries.iter().find(|v| v["loader"]["stable"].as_bool().unwrap_or(true))
                .or_else(|| entries.first())
                .and_then(|v| v["loader"]["version"].as_str())
                .ok_or_else(|| anyhow!("Нет совместимой версии {loader} для {game_version}"))?.to_owned()
        } else {
            config.selected_loader_version.trim().to_owned()
        };
        let profile_url = format!("{api}/{}/{}/profile/json",
            url_encode_component(game_version), url_encode_component(&loader_version));
        let mut profile: Value = client.get(profile_url).send()?.error_for_status()?.json()?;
        if profile["inheritsFrom"].as_str() != Some(game_version) {
            bail!("Профиль {loader} несовместим с Minecraft {game_version}");
        }
        if profile["id"].as_str().unwrap_or("").is_empty() {
            profile["id"] = Value::String(format!("{}-{}-{}", loader.to_lowercase(), loader_version, game_version));
        }
        let id = profile["id"].as_str().unwrap().to_owned();
        validate_version_id(&id)?;
        let directory = game_dir.join("versions").join(&id);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join(format!("{id}.json")), serde_json::to_vec_pretty(&profile)?)?;
        status(events, &format!("{loader} {loader_version} установлен"));
        return Ok(id);
    }

    let loader_version = if !config.selected_loader_version.trim().is_empty() {
        config.selected_loader_version.trim().to_owned()
    } else if loader == "Forge" {
        resolve_forge_version(client, game_version)?
    } else {
        resolve_neoforge_version(client, game_version)?
    };
    let installer_url = if loader == "Forge" {
        let combined = format!("{game_version}-{loader_version}");
        format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{combined}/forge-{combined}-installer.jar")
    } else {
        format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar")
    };
    let installer_dir = game_dir.join(".minty").join("installers");
    fs::create_dir_all(&installer_dir)?;
    let installer_path = installer_dir.join(format!("{}-{}-installer.jar",
        loader.to_lowercase(), loader_version.replace(['/', '\\', ':'], "_")));
    status(events, &format!("Скачиваю официальный установщик {loader} {loader_version}…"));
    download_verified(client, &installer_url, &installer_path, None)?;
    let java_major = installer_java_version(game_version);
    let java = runtime::ensure_java(java_major, "", &crate::config::data_dir()?.join("runtimes"))?;
    status(events, &format!("Устанавливаю {loader} для Minecraft {game_version}…"));
    let output = Command::new(&java).arg("-jar").arg(&installer_path)
        .arg("--installClient").arg(game_dir).output()
        .with_context(|| format!("Не удалось запустить установщик {loader}"))?;
    if !output.status.success() {
        bail!("Установщик {loader} завершился с кодом {:?}.\n{}\n{}",
            output.status.code(), String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr));
    }
    find_loader_profile(game_dir, game_version, loader)
}

fn installer_java_version(game_version: &str) -> u32 {
    let parts: Vec<u32> = game_version.split('.').filter_map(|v| v.parse().ok()).collect();
    match (*parts.get(1).unwrap_or(&0), *parts.get(2).unwrap_or(&0)) {
        (0..=16, _) => 8, (17, _) => 16, (18..=20, 0..=4) => 17, _ => 21,
    }
}

fn resolve_forge_version(client: &Client, game_version: &str) -> Result<String> {
    let json: Value = client.get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
        .send()?.error_for_status()?.json()?;
    let promos = json["promos"].as_object().ok_or_else(|| anyhow!("Forge promotions JSON повреждён"))?;
    let recommended = format!("{game_version}-recommended");
    let latest = format!("{game_version}-latest");
    promos.get(&recommended).and_then(Value::as_str)
        .or_else(|| promos.get(&latest).and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Forge не публикует установщик для Minecraft {game_version}"))
}

fn resolve_neoforge_version(client: &Client, game_version: &str) -> Result<String> {
    let parts: Vec<u32> = game_version.split('.').filter_map(|v| v.parse().ok()).collect();
    if parts.len() < 2 || parts[0] != 1 { bail!("Не удалось сопоставить Minecraft {game_version} с NeoForge"); }
    let prefix = format!("{}.{}.", parts[1], parts.get(2).copied().unwrap_or(0));
    let xml = client.get("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml")
        .send()?.error_for_status()?.text()?;
    let mut versions: Vec<String> = xml.split("<version>").skip(1)
        .filter_map(|v| v.split("</version>").next())
        .filter(|v| v.starts_with(&prefix) && !v.contains("beta"))
        .map(str::to_owned).collect();
    versions.sort_by_key(|v| v.split('.').map(|n| n.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>());
    versions.pop().ok_or_else(|| anyhow!("NeoForge не публикует установщик для Minecraft {game_version}"))
}

fn find_loader_profile(game_dir: &Path, game_version: &str, loader: &str) -> Result<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = Vec::new();
    for entry in fs::read_dir(game_dir.join("versions"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() { continue; }
        let Some(folder) = entry.file_name().to_str().map(str::to_owned) else { continue; };
        let path = entry.path().join(format!("{folder}.json"));
        let Ok(bytes) = fs::read(&path) else { continue; };
        let Ok(profile) = serde_json::from_slice::<Value>(&bytes) else { continue; };
        let id = profile["id"].as_str().unwrap_or(&folder).to_owned();
        if profile["inheritsFrom"].as_str() != Some(game_version)
            || !id.to_ascii_lowercase().contains(&loader.to_ascii_lowercase()) { continue; }
        let time = fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        found.push((time, id));
    }
    found.sort_by(|a,b| b.0.cmp(&a.0));
    found.into_iter().next().map(|(_,id)| id)
        .ok_or_else(|| anyhow!("Установщик {loader} не создал профиль для Minecraft {game_version}"))
}

fn merge_loader_profile(base: &Value, profile: &Value) -> Result<Value> {
    if let Some(parent) = profile["inheritsFrom"].as_str() {
        if base["id"].as_str() != Some(parent) {
            bail!("Профиль наследует Minecraft {parent}, а выбрана версия {}", base["id"].as_str().unwrap_or("unknown"));
        }
    }
    let mut merged = base.clone();
    let mut libraries = merged["libraries"].as_array().cloned().unwrap_or_default();
    if let Some(extra) = profile["libraries"].as_array() {
        for library in extra {
            let name = library["name"].as_str();
            if !libraries.iter().any(|item| name.is_some() && item["name"].as_str() == name) {
                libraries.push(library.clone());
            }
        }
    }
    merged["libraries"] = Value::Array(libraries);
    for key in ["mainClass", "minecraftArguments", "logging", "type"] {
        if let Some(value) = profile.get(key) { merged[key] = value.clone(); }
    }
    if let Some(groups) = profile.get("arguments").and_then(Value::as_object) {
        if !merged["arguments"].is_object() { merged["arguments"] = serde_json::json!({}); }
        for group in ["jvm", "game"] {
            if let Some(extra) = groups.get(group).and_then(Value::as_array) {
                if !merged["arguments"][group].is_array() { merged["arguments"][group] = Value::Array(Vec::new()); }
                merged["arguments"][group].as_array_mut().unwrap().extend(extra.iter().cloned());
            }
        }
    }
    if let Some(id) = profile["id"].as_str() { merged["id"] = Value::String(id.to_owned()); }
    Ok(merged)
}

fn resolve_library_artifact(library: &Value, libraries_dir: &Path) -> Result<Option<(PathBuf, String, Option<String>)>> {
    if let Some(artifact) = library.get("downloads").and_then(|v| v.get("artifact")) {
        if let Some(info) = download_info(artifact) {
            if let Some(path) = info.path.as_deref() {
                return Ok(Some((libraries_dir.join(path), info.url, info.sha1)));
            }
        }
    }
    let Some(name) = library["name"].as_str() else { return Ok(None); };
    let (coordinates, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coordinates.split(':').collect();
    if parts.len() < 3 || parts.len() > 4
        || parts.iter().any(|v| v.is_empty() || v.contains("..") || v.contains('/') || v.contains('\\')) {
        return Ok(None);
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|v| format!("-{v}")).unwrap_or_default();
    let relative = format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}");
    let base = library["url"].as_str().unwrap_or("https://libraries.minecraft.net/");
    let url = format!("{}/{}", base.trim_end_matches('/'), relative);
    Ok(Some((libraries_dir.join(relative), url, None)))
}

fn url_encode_component(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') { output.push(byte as char); }
        else { output.push_str(&format!("%{byte:02X}")); }
    }
    output
}

fn download_info(value: &Value) -> Option<DownloadInfo> {
    serde_json::from_value(value.clone()).ok()
}

fn library_allowed(library: &Value) -> bool {
    let Some(rules) = library.get("rules").and_then(Value::as_array) else { return true; };
    let mut allowed = false;
    for rule in rules {
        let os = rule.get("os");
        let os_matches = os.is_none() || os.map(|o| {
            o.get("name").and_then(Value::as_str).map(|name| name == "windows").unwrap_or(true)
                && o.get("arch").and_then(Value::as_str).map(|arch| arch == "x86_64" || arch == "amd64").unwrap_or(true)
        }).unwrap_or(true);
        if os_matches {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn native_download(library: &Value) -> Option<DownloadInfo> {
    let downloads = library.get("downloads")?;
    let classifiers = downloads.get("classifiers")?.as_object()?;
    let natives_name = library.get("natives")?.get("windows")?.as_str()?;
    let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
    let classifier = natives_name.replace("${arch}", arch);
    classifiers.get(&classifier).and_then(download_info)
}

fn extract_natives(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).context("Некорректный архив native-библиотеки")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
        if entry.is_dir() || relative.components().any(|c| c.as_os_str() == "META-INF") { continue; }
        let output = destination.join(relative);
        if let Some(parent) = output.parent() { fs::create_dir_all(parent)?; }
        let mut out = fs::File::create(output)?;
        std::io::copy(&mut entry, &mut out)?;
        out.flush()?;
    }
    Ok(())
}

fn parse_arguments(arguments: &[Value], vars: &HashMap<&str, &str>) -> Result<Vec<String>> {
    let mut parsed = Vec::new();
    for item in arguments {
        if let Some(text) = item.as_str() {
            parsed.push(expand(text, vars)?);
            continue;
        }
        let rules = item.get("rules").and_then(Value::as_array);
        if let Some(rules) = rules {
            if !argument_rules_allow(rules) { continue; }
        }
        let value = &item["value"];
        if let Some(text) = value.as_str() {
            parsed.push(expand(text, vars)?);
        } else if let Some(array) = value.as_array() {
            for part in array.iter().filter_map(Value::as_str) { parsed.push(expand(part, vars)?); }
        }
    }
    Ok(parsed)
}

fn argument_rules_allow(rules: &[Value]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os = rule.get("os");
        let os_matches = os.is_none() || os.map(|o| {
            o.get("name").and_then(Value::as_str).map(|s| s == "windows").unwrap_or(true)
                && o.get("arch").and_then(Value::as_str).map(|s| s == "x86_64" || s == "amd64").unwrap_or(true)
        }).unwrap_or(true);
        let features_match = rule.get("features").and_then(Value::as_object).map(|features| {
            features.iter().all(|(name, expected)| {
                let actual = match name.as_str() {
                    "is_demo_user" | "has_custom_resolution" | "has_quick_plays_support" => Some(false),
                    _ => None,
                };
                actual.map(|value| expected.as_bool() == Some(value)).unwrap_or(false)
            })
        }).unwrap_or(true);
        if os_matches && features_match { allowed = rule["action"].as_str() == Some("allow"); }
    }
    allowed
}

fn expand(input: &str, vars: &HashMap<&str, &str>) -> Result<String> {
    let mut result = input.to_owned();
    for (key, value) in vars {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    if result.contains("${") {
        // Unknown launcher variables should not silently reach the game as broken paths/tokens.
        let remaining = result.split("${").nth(1).unwrap_or_default();
        if let Some(name) = remaining.split('}').next() {
            if !name.starts_with("classpath") && !name.starts_with("library_directory") {
                // Some Mojang metadata can introduce optional newer variables. Keep these visible in the error.
                bail!("Неизвестная переменная аргумента Minecraft: ${{{name}}}");
            }
        }
    }
    Ok(result)
}

fn offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let value = hex::encode(bytes);
    format!("{}-{}-{}-{}-{}", &value[0..8], &value[8..12], &value[12..16], &value[16..20], &value[20..32])
}


#[cfg(test)]
mod tests {
    use super::apply_weak_pc_profile;
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("minty-test-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn weak_profile_restores_original_options() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let options = dir.join("options.txt");
        let original = "renderDistance:16\ngraphicsMode:2\ncustomKey:hello\n";
        fs::write(&options, original).unwrap();

        apply_weak_pc_profile(&dir, true).unwrap();
        let optimized = fs::read_to_string(&options).unwrap();
        assert!(optimized.contains("renderDistance:6"));
        assert!(optimized.contains("graphicsMode:0"));
        assert!(optimized.contains("customKey:hello"));

        apply_weak_pc_profile(&dir, false).unwrap();
        assert_eq!(fs::read_to_string(&options).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn weak_profile_removes_first_run_generated_options_when_disabled() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let options = dir.join("options.txt");
        apply_weak_pc_profile(&dir, true).unwrap();
        assert!(options.is_file());
        apply_weak_pc_profile(&dir, false).unwrap();
        assert!(!options.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}

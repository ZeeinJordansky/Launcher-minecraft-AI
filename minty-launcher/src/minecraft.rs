use crate::{auth::Account, config::AppConfig, runtime};
use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::Sender;
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, fs, io::{Read, Write}, path::{Path, PathBuf}, process::Command, sync::{atomic::{AtomicUsize, Ordering}, Arc}, time::Duration};
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
    Finished(String),
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
    let versions = manifest.versions.into_iter().filter(|v| v.kind == "release").collect();
    Ok((versions, latest))
}

pub fn launch(config: AppConfig, account: Account, events: Sender<LaunchEvent>) -> Result<()> {
    let result = launch_inner(&config, &account, &events);
    match result {
        Ok(code) => { events.send(LaunchEvent::Finished(format!("Minecraft завершил работу (код {code})."))).ok(); Ok(()) }
        Err(error) => { events.send(LaunchEvent::Failed(format!("{error:#}"))).ok(); Err(error) }
    }
}

fn launch_inner(config: &AppConfig, account: &Account, events: &Sender<LaunchEvent>) -> Result<i32> {
    let client = http_client()?;
    let version = if config.selected_version == "latest-release" {
        status(events, "Проверяю список версий…");
        fetch_versions()?.1
    } else { config.selected_version.clone() };
    validate_version_id(&version)?;

    let game_dir = config.game_dir.clone();
    let versions_dir = game_dir.join("versions").join(&version);
    let libraries_dir = game_dir.join("libraries");
    let assets_dir = game_dir.join("assets");
    let natives_dir = game_dir.join("natives").join(&version);
    fs::create_dir_all(&game_dir)?;
    apply_weak_pc_profile(&game_dir, config.weak_pc_mode)?;
    for dir in [&versions_dir, &libraries_dir, &assets_dir, &natives_dir] { fs::create_dir_all(dir)?; }

    status(events, &format!("Получаю метаданные Minecraft {version}…"));
    let manifest: VersionManifest = client.get(VERSION_MANIFEST).send()?.error_for_status()?.json()?;
    let version_info = manifest.versions.iter().find(|v| v.id == version)
        .ok_or_else(|| anyhow!("Версия {version} отсутствует в официальном манифесте Mojang"))?;
    let metadata_path = versions_dir.join(format!("{version}.json"));
    download_verified(&client, &version_info.url, &metadata_path, version_info.sha1.as_deref())?;
    let metadata: Value = serde_json::from_slice(&fs::read(&metadata_path)?)?;

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
            if !library_allowed(library) { continue; }
            if let Some(artifact) = library.get("downloads").and_then(|d| d.get("artifact")) {
                if let Some(info) = download_info(artifact) {
                    let relative = info.path.as_deref().ok_or_else(|| anyhow!("В библиотеке нет пути файла"))?;
                    let path = libraries_dir.join(relative);
                    download_verified(&client, &info.url, &path, info.sha1.as_deref())?;
                    classpath.push(path);
                }
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
    for archive in native_archives { extract_natives(&archive, &natives_dir)?; }

    let asset_index = metadata["assetIndex"].clone();
    let asset_index_id = asset_index["id"].as_str().ok_or_else(|| anyhow!("В метаданных отсутствует assetIndex.id"))?;
    let asset_index_url = asset_index["url"].as_str().ok_or_else(|| anyhow!("В метаданных отсутствует assetIndex.url"))?;
    let index_path = assets_dir.join("indexes").join(format!("{asset_index_id}.json"));
    status(events, "Проверяю индекс ресурсов…");
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
        let path = asset_root.join(&hash[..2]).join(hash);
        let result = download_verified(&client, url, &path, Some(hash));
        completed.fetch_add(1, Ordering::Relaxed);
        result.err().map(|e| format!("{hash}: {e}"))
    }).collect();
    if !errors.is_empty() {
        bail!("Не удалось загрузить {} ресурсов. Первый сбой: {}", errors.len(), errors[0]);
    }

    let java_major = metadata["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    status(events, &format!("Проверяю Java {java_major}…"));
    let runtime_root = crate::config::data_dir()?.join("runtimes");
    let java = runtime::ensure_java(java_major, &config.java_path, &runtime_root)?;

    let main_class = metadata["mainClass"].as_str().ok_or_else(|| anyhow!("Не указана mainClass в метаданных Minecraft"))?;
    let mut variables = HashMap::new();
    variables.insert("auth_player_name", account.name.as_str());
    variables.insert("version_name", version.as_str());
    let game_dir_text = game_dir.to_string_lossy().to_string();
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
    variables.insert("version_type", "release");
    variables.insert("assets_index_name", asset_index_id);
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
    let max_memory_gb = if config.weak_pc_mode {
        2
    } else {
        config.max_memory_gb.clamp(2, 16)
    };
    jvm_args.push(if config.weak_pc_mode { "-Xms512M".to_owned() } else { "-Xms1G".to_owned() });
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

    if config.weak_pc_mode {
        jvm_args.push("-XX:+UseG1GC".to_owned());
        jvm_args.push("-XX:MaxGCPauseMillis=100".to_owned());
        jvm_args.push("-XX:+ParallelRefProcEnabled".to_owned());
    }

    status(events, "Запускаю Minecraft через Java bootstrap…");
    let bootstrap_dir = crate::config::data_dir()?.join("bootstrap");
    fs::create_dir_all(&bootstrap_dir)?;
    let bootstrap_class = bootstrap_dir.join("MintyBootstrap.class");
    fs::write(&bootstrap_class, include_bytes!(env!("MINTY_BOOTSTRAP_CLASS")))?;

    let mut command = Command::new(&java);
    command.arg("-cp").arg(&bootstrap_dir).arg("MintyBootstrap").arg(&java);
    for arg in jvm_args.into_iter().chain(std::iter::once(main_class.to_owned())).chain(game_args) {
        command.arg(arg);
    }
    command.current_dir(&game_dir);
    let status = command.status().context("Не удалось запустить Java bootstrap")?;
    Ok(status.code().unwrap_or(-1))
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

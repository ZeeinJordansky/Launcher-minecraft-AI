use anyhow::{Context, Result};
use chrono::Local;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub game_dir: PathBuf,
    pub selected_version: String,
    pub selected_loader: String,
    pub selected_loader_version: String,
    pub selected_profile_id: String,
    pub offline_username: String,
    pub microsoft_client_id: String,
    pub java_path: String,
    pub max_memory_gb: u8,
    pub auto_memory: bool,
    pub auto_graphics_profile: bool,
    pub weak_pc_mode: bool,
    pub launch_fullscreen: bool,
    pub gc_algorithm: String,
    pub isolated_profiles: bool,
    pub bedrock_channel: String,
    pub enable_animations: bool,
    pub ui_scale: f32,
    pub language: String,
    pub check_updates_on_start: bool,
    pub close_after_game: bool,
    pub skipped_update_version: Option<String>,
    pub accent_color: [u8; 3],
    pub last_account_name: Option<String>,
    pub last_account_uuid: Option<String>,
    pub last_account_xuid: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let game_dir = dirs_home().join(".minecraft");
        Self {
            game_dir,
            selected_version: "latest-release".to_owned(),
            selected_loader: "Vanilla".to_owned(),
            selected_loader_version: String::new(),
            selected_profile_id: String::new(),
            offline_username: "Player".to_owned(),
            microsoft_client_id: String::new(),
            java_path: String::new(),
            max_memory_gb: 4,
            auto_memory: true,
            auto_graphics_profile: true,
            weak_pc_mode: false,
            launch_fullscreen: false,
            gc_algorithm: "Авто".to_owned(),
            isolated_profiles: false,
            bedrock_channel: "Стабильная".to_owned(),
            enable_animations: true,
            ui_scale: 1.0,
            language: "Русский".to_owned(),
            check_updates_on_start: true,
            close_after_game: false,
            skipped_update_version: None,
            accent_color: [105, 220, 178],
            last_account_name: None,
            last_account_uuid: None,
            last_account_xuid: None,
        }
    }
}

fn dirs_home() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the mutable data directory for the active game profile.
///
/// Shared versions, libraries, assets and natives stay in game_dir. When isolation
/// is enabled, worlds, mods, options and resource packs live in their own folder.
pub fn instance_dir(config: &AppConfig) -> PathBuf {
    if !config.isolated_profiles {
        return config.game_dir.clone();
    }
    let profile = if config.selected_profile_id.trim().is_empty() {
        config.selected_version.as_str()
    } else {
        config.selected_profile_id.as_str()
    };
    let safe_name: String = profile.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            character
        } else {
            '_'
        }
    }).collect();
    config.game_dir.join("instances").join(safe_name)
}

/// Prepares the active instance and migrates existing mutable game data once.
///
/// Files already present in the destination are never overwritten. Symlinks in
/// the old game directory are skipped so the migration cannot follow them outside
/// the selected Minecraft folder.
pub fn prepare_instance_dir(config: &AppConfig) -> Result<PathBuf> {
    let target = instance_dir(config);
    fs::create_dir_all(&target)?;

    if !config.isolated_profiles {
        return Ok(target);
    }

    for name in [
        "mods", "saves", "resourcepacks", "shaderpacks", "config",
        "screenshots", "server-resource-packs", "crash-reports",
    ] {
        let source = config.game_dir.join(name);
        let destination = target.join(name);
        // Merge missing files even when the profile folder was created earlier by
        // the mod manager; existing profile files are never overwritten.
        if source.is_dir() {
            copy_instance_directory(&source, &destination)?;
        }
    }

    for name in [
        "options.txt", "servers.dat", "servers.dat_old", "optionsof.txt",
        "optionsshaders.txt", "optionsmenu.txt", "optionsvivecraft.txt",
    ] {
        let source = config.game_dir.join(name);
        let destination = target.join(name);
        if source.is_file() && !destination.exists() {
            fs::copy(source, destination)?;
        }
    }

    Ok(target)
}

fn copy_instance_directory(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if !target_path.exists() {
                fs::create_dir_all(&target_path)?;
            }
            if target_path.is_dir() {
                copy_instance_directory(&source_path, &target_path)?;
            }
        } else if metadata.is_file() && !target_path.exists() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

pub fn config_file() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "Minty", "Minty Launcher")
        .context("Не удалось определить каталог конфигурации")?;
    fs::create_dir_all(dirs.config_dir())?;
    Ok(dirs.config_dir().join("settings.json"))
}

pub fn data_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "Minty", "Minty Launcher")
        .context("Не удалось определить каталог данных")?;
    fs::create_dir_all(dirs.data_local_dir())?;
    Ok(dirs.data_local_dir().to_path_buf())
}

pub fn log_file() -> Result<PathBuf> {
    let dir = data_dir()?.join("logs");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("launcher.log"))
}

pub fn append_log(message: &str) -> Result<()> {
    let path = log_file()?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "[{}] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), message)?;
    Ok(())
}

pub fn recent_logs(limit: usize) -> Result<Vec<String>> {
    let path = log_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path)?;
    Ok(contents
        .lines()
        .rev()
        .take(limit)
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

pub fn clear_logs() -> Result<()> {
    fs::write(log_file()?, "")?;
    Ok(())
}

pub fn load() -> Result<AppConfig> {
    let path = config_file()?;
    if !path.exists() {
        let config = AppConfig::default();
        save(&config)?;
        return Ok(config);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Не удалось прочитать {}", path.display()))?;
    let config = serde_json::from_str(&raw)
        .context("Файл настроек повреждён или имеет несовместимый формат")?;
    Ok(config)
}

pub fn save(config: &AppConfig) -> Result<()> {
    let path = config_file()?;
    let raw = serde_json::to_string_pretty(config)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, raw)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}


#[cfg(test)]
mod instance_tests {
    use super::{instance_dir, prepare_instance_dir, AppConfig};
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    fn temp_root() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("minty-instance-test-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn isolation_uses_a_stable_profile_folder() {
        let mut config = AppConfig::default();
        config.game_dir = PathBuf::from("C:/Games/Minecraft");
        config.isolated_profiles = true;
        config.selected_profile_id = "fabric-loader-0.16.0-1.21.1".to_owned();
        assert!(instance_dir(&config).ends_with("instances/fabric-loader-0_16_0-1_21_1"));
    }

    #[test]
    fn enabling_isolation_copies_existing_saves_and_settings_without_overwriting() {
        let root = temp_root();
        let mut config = AppConfig::default();
        config.game_dir = root.clone();
        config.isolated_profiles = true;
        config.selected_profile_id = "1.21.1".to_owned();

        fs::create_dir_all(root.join("saves").join("World")).unwrap();
        fs::write(root.join("saves").join("World").join("level.dat"), b"world").unwrap();
        fs::write(root.join("options.txt"), b"renderDistance:12\n").unwrap();

        let target = prepare_instance_dir(&config).unwrap();
        assert_eq!(fs::read(target.join("saves/World/level.dat")).unwrap(), b"world".to_vec());
        assert_eq!(fs::read(target.join("options.txt")).unwrap(), b"renderDistance:12\n");

        fs::write(target.join("options.txt"), b"renderDistance:8\n").unwrap();
        prepare_instance_dir(&config).unwrap();
        assert_eq!(fs::read(target.join("options.txt")).unwrap(), b"renderDistance:8\n");
        fs::remove_dir_all(root).unwrap();
    }
}

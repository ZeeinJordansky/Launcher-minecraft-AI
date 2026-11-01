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
    pub weak_pc_mode: bool,
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
            weak_pc_mode: false,
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

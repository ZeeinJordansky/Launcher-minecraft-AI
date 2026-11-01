use anyhow::{anyhow, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct BedrockData {
    pub channel: &'static str,
    pub path: PathBuf,
}

pub fn installations() -> Vec<BedrockData> {
    let mut result = Vec::new();
    if let Some(path) = com_mojang_dir(false) {
        result.push(BedrockData { channel: "Minecraft Bedrock", path });
    }
    if let Some(path) = com_mojang_dir(true) {
        result.push(BedrockData { channel: "Minecraft Preview", path });
    }
    result
}

pub fn com_mojang_dir(preview: bool) -> Option<PathBuf> {
    let roaming = env::var_os("APPDATA").map(PathBuf::from)?;
    let root = roaming.join(if preview { "Minecraft Bedrock Preview" } else { "Minecraft Bedrock" });
    let users = root.join("Users");
    let shared = users.join("Shared").join("games").join("com.mojang");
    if shared.is_dir() { return Some(shared); }

    if let Ok(entries) = fs::read_dir(&users) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("games").join("com.mojang");
            if candidate.is_dir() { return Some(candidate); }
        }
    }

    // Fallback for legacy UWP installs that predate the Windows GDK data layout.
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let packages = local.join("Packages");
    let prefix = if preview { "Microsoft.MinecraftWindowsBeta_" } else { "Microsoft.MinecraftUWP_" };
    if let Ok(entries) = fs::read_dir(packages) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) {
                let candidate = entry.path().join("LocalState").join("games").join("com.mojang");
                if candidate.is_dir() { return Some(candidate); }
            }
        }
    }
    None
}

pub fn launch() -> Result<()> {
    open::that("minecraft:")
        .context("Не удалось открыть Minecraft Bedrock. Проверь, что Minecraft for Windows установлен из Microsoft Store.")
}

pub fn import_addon(path: &Path) -> Result<()> {
    let extension = path.extension().and_then(|v| v.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !matches!(extension.as_str(), "mcpack" | "mcaddon" | "mcworld") {
        return Err(anyhow!("Поддерживаются .mcpack, .mcaddon и .mcworld."));
    }
    if !path.is_file() {
        return Err(anyhow!("Файл дополнения не найден: {}", path.display()));
    }
    // Microsoft documents importing these package types by opening the downloaded file;
    // the registered Minecraft protocol/file association handles the final import.
    open::that(path).with_context(|| format!("Windows не смог передать файл Minecraft: {}", path.display()))
}

pub fn open_data_folder(preview: bool) -> Result<()> {
    let path = com_mojang_dir(preview)
        .ok_or_else(|| anyhow!("Папка данных Bedrock пока не найдена. Сначала запусти Minecraft Bedrock хотя бы один раз."))?;
    open::that(path).context("Не удалось открыть папку данных Bedrock")
}

pub fn open_store_search() -> Result<()> {
    open::that("ms-windows-store://search/?query=Minecraft%20for%20Windows")
        .context("Не удалось открыть Microsoft Store")
}

pub fn installed_data_hint() -> &'static str {
    "Bedrock получает обновления и версии через Microsoft Store. Minty открывает установленную игру и передаёт ей файлы дополнений."
}
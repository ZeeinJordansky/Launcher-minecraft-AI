use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Deserialize)]
pub struct ModProject {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectVersion {
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    files: Vec<ProjectFile>,
}

#[derive(Debug, Deserialize)]
struct ProjectFile {
    filename: String,
    url: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
}

pub fn search(query: &str, game_version: &str, loader: &str) -> Result<Vec<ModProject>> {
    let client = client()?;
    let mut facets = vec![vec!["project_type:mod".to_owned()]];
    if !game_version.trim().is_empty() {
        facets.push(vec![format!("versions:{game_version}")]);
    }
    let loader_facet = loader_facet(loader);
    if let Some(value) = loader_facet {
        facets.push(vec![format!("categories:{value}")]);
    }
    let facets = serde_json::to_string(&facets)?;
    let response = client.get(format!("{API}/search"))
        .query(&[
            ("query", query),
            ("limit", "30"),
            ("index", "downloads"),
            ("facets", facets.as_str()),
        ])
        .send()
        .context("Не удалось подключиться к каталогу Modrinth")?
        .error_for_status()
        .context("Modrinth не выполнил поиск модов")?;
    #[derive(Deserialize)]
    struct SearchResults { hits: Vec<ModProject> }
    let result: SearchResults = response.json().context("Modrinth вернул некорректный каталог модов")?;
    Ok(result.hits)
}

pub fn install(slug: &str, game_version: &str, loader: &str, mods_dir: &Path) -> Result<PathBuf> {
    if loader.eq_ignore_ascii_case("Vanilla") {
        bail!("Для установки модов сначала выбери Fabric, Forge, NeoForge или Quilt.");
    }
    let loader_id = loader.to_ascii_lowercase();
    let client = client()?;
    let game_versions = serde_json::to_string(&vec![game_version])?;
    let loaders = serde_json::to_string(&vec![loader_id.clone()])?;
    let versions: Vec<ProjectVersion> = client.get(format!("{API}/project/{slug}/version"))
        .query(&[("game_versions", game_versions.as_str()), ("loaders", loaders.as_str())])
        .send()
        .context("Не удалось получить версии мода")?
        .error_for_status()
        .context("Modrinth не нашёл совместимую версию мода")?
        .json()
        .context("Modrinth вернул некорректные сведения о версии мода")?;

    let version = versions.iter().find(|version| {
        version.game_versions.iter().any(|value| value == game_version)
            && version.loaders.iter().any(|value| value.eq_ignore_ascii_case(&loader_id))
    }).or_else(|| versions.first())
        .ok_or_else(|| anyhow!("У мода нет сборки для Minecraft {game_version} и {loader}."))?;
    let file = version.files.iter().find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| anyhow!("В версии мода отсутствует файл для загрузки"))?;
    let filename = file.filename.rsplit(['/', '\\']).next().unwrap_or("mod.jar");
    if !filename.to_ascii_lowercase().ends_with(".jar") {
        bail!("Modrinth вернул файл не формата .jar: {filename}");
    }

    fs::create_dir_all(mods_dir)
        .with_context(|| format!("Не удалось создать папку модов: {}", mods_dir.display()))?;
    let mut target = mods_dir.join(filename);
    if target.exists() {
        let stem = Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or("mod");
        target = mods_dir.join(format!("{stem}-{slug}.jar"));
    }

    let response = client.get(&file.url).send()
        .context("Не удалось скачать файл мода")?
        .error_for_status()
        .context("Modrinth отклонил скачивание мода")?;
    let bytes = response.bytes().context("Не удалось прочитать скачанный мод")?;
    if bytes.len() < 128 || !bytes.starts_with(b"PK") {
        bail!("Скачанный файл не похож на JAR/ZIP-архив мода.");
    }
    if let Some(expected) = file.hashes.get("sha1") {
        let actual = hex::encode(Sha1::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            bail!("Контрольная сумма мода не совпала. Файл не установлен.");
        }
    }
    fs::write(&target, &bytes)
        .with_context(|| format!("Не удалось записать мод в {}", target.display()))?;
    Ok(target)
}

fn loader_facet(loader: &str) -> Option<&'static str> {
    match loader.to_ascii_lowercase().as_str() {
        "fabric" => Some("fabric"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        "quilt" => Some("quilt"),
        _ => None,
    }
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("ZeeinJordansky/MintyLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::loader_facet;
    #[test]
    fn maps_supported_loaders_to_modrinth_categories() {
        assert_eq!(loader_facet("Fabric"), Some("fabric"));
        assert_eq!(loader_facet("NeoForge"), Some("neoforge"));
        assert_eq!(loader_facet("Vanilla"), None);
    }
}
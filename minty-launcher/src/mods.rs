use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone)]
pub struct ModIssue {
    pub title: String,
    pub details: String,
    pub critical: bool,
}

/// Scan local JAR archives for invalid ZIP files, duplicate IDs and mismatched loader metadata.
/// This is static inspection only: it does not execute mods or resolve every dependency.
pub fn analyze(mods_dir: &Path, loader: &str) -> Vec<ModIssue> {
    let mut issues = Vec::new();
    let entries = match fs::read_dir(mods_dir) {
        Ok(entries) => entries,
        Err(_) => return issues,
    };
    let mut seen_ids: HashMap<String, String> = HashMap::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let is_jar = path.extension().and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("jar")).unwrap_or(false);
        if !path.is_file() || !is_jar {
            continue;
        }
        let filename = path.file_name().and_then(|value| value.to_str())
            .unwrap_or("mod.jar").to_owned();
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) => {
                issues.push(ModIssue {
                    title: format!("Не удалось прочитать {filename}"),
                    details: error.to_string(),
                    critical: true,
                });
                continue;
            }
        };
        let mut archive = match zip::ZipArchive::new(file) {
            Ok(archive) => archive,
            Err(error) => {
                issues.push(ModIssue {
                    title: format!("Повреждённый архив: {filename}"),
                    details: error.to_string(),
                    critical: true,
                });
                continue;
            }
        };

        let fabric = read_manifest(&mut archive, "fabric.mod.json");
        let quilt = read_manifest(&mut archive, "quilt.mod.json");
        let forge = read_manifest(&mut archive, "META-INF/mods.toml");
        let neoforge = read_manifest(&mut archive, "META-INF/neoforge.mods.toml");

        let (mod_id, required_loader) = if let Some(raw) = fabric {
            let id = serde_json::from_str::<serde_json::Value>(&raw).ok()
                .and_then(|json| json.get("id").and_then(|value| value.as_str()).map(str::to_owned));
            (id, Some("Fabric"))
        } else if let Some(raw) = quilt {
            let id = serde_json::from_str::<serde_json::Value>(&raw).ok()
                .and_then(|json| json.get("quilt_loader").and_then(|value| value.get("id"))
                    .and_then(|value| value.as_str()).map(str::to_owned));
            (id, Some("Quilt"))
        } else if forge.is_some() {
            (None, Some("Forge"))
        } else if neoforge.is_some() {
            (None, Some("NeoForge"))
        } else {
            (None, None)
        };

        if let Some(required) = required_loader {
            let compatible = required.eq_ignore_ascii_case(loader);
            if !compatible {
                issues.push(ModIssue {
                    title: format!("{filename}: требуется {required}"),
                    details: format!("Выбран загрузчик {loader}; JAR содержит метаданные {required}."),
                    critical: true,
                });
            }
        }

        if let Some(id) = mod_id {
            if let Some(previous) = seen_ids.insert(id.clone(), filename.clone()) {
                issues.push(ModIssue {
                    title: format!("Повторяется мод {id}"),
                    details: format!("ID встречается в файлах {previous} и {filename}. Проверь, не установлены ли две версии одного мода."),
                    critical: true,
                });
            }
        }
    }

    if issues.is_empty() && mods_dir.is_dir() {
        issues.push(ModIssue {
            title: "Очевидные конфликты не найдены".to_owned(),
            details: "Проверены целостность JAR/ZIP, часть метаданных и повторяющиеся mod ID. Все зависимости и конфликты во время игры эта проверка не исключает.".to_owned(),
            critical: false,
        });
    }
    issues
}

fn read_manifest(archive: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut content = String::new();
    entry.read_to_string(&mut content).ok()?;
    Some(content)
}

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
    #[serde(default)]
    pub project_type: String,
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

pub fn search(query: &str, game_version: &str, loader: &str, project_type: &str, sort_index: &str) -> Result<Vec<ModProject>> {
    let client = client()?;
    let project_type = match project_type {
        "modpack" => "modpack",
        "resourcepack" => "resourcepack",
        "shader" => "shader",
        _ => "mod",
    };
    let mut facets = vec![vec![format!("project_type:{project_type}")]];
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
            ("index", if matches!(sort_index, "relevance" | "downloads" | "follows" | "newest" | "updated") { sort_index } else { "downloads" }),
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
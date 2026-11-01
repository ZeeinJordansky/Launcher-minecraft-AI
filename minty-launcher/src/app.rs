use crate::{
    auth::{self, Account, AuthEvent},
    config::{self, AppConfig},
    minecraft::{self, LaunchEvent, VersionInfo},
    skin,
    mods::{self, ModProject},
    bedrock,
    system_info::{self, HardwareInfo},
    updater::{self, UpdateInfo},
    crash,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Rounding, Stroke};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread,
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(13, 17, 22);
const SIDEBAR: Color32 = Color32::from_rgb(18, 24, 31);
const PANEL: Color32 = Color32::from_rgb(24, 32, 41);
const PANEL_LIGHT: Color32 = Color32::from_rgb(34, 44, 55);
const MINT: Color32 = Color32::from_rgb(145, 226, 183);
const MINT_DARK: Color32 = Color32::from_rgb(50, 122, 94);
const TEXT: Color32 = Color32::from_rgb(239, 243, 239);
const MUTED: Color32 = Color32::from_rgb(153, 169, 179);
const RED: Color32 = Color32::from_rgb(235, 111, 115);
const GOLD: Color32 = Color32::from_rgb(234, 192, 117);

#[derive(PartialEq, Eq, Clone, Copy)]
enum BootStage { CheckingUpdates, SearchingJava, Loading, Ready }

#[derive(Clone)]
struct InstalledProfile { id: String, game_version: String, loader: String }

#[derive(PartialEq, Eq, Clone, Copy)]
enum Page {
    Home,
    Minecraft,
    Settings,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum MinecraftTab {
    Builds,
    Versions,
    Profile,
    Mods,
    Bedrock,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum SettingsTab {
    General, Game, Java, Performance, Appearance, Account, Storage, Security, Updates, Journal,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum SkinTool { Brush, Eraser, Fill, Eyedropper }

struct AvatarImage {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

pub struct MintyApp {
    config: AppConfig,
    hardware: HardwareInfo,
    avatar_tx: Sender<Result<AvatarImage, String>>,
    avatar_rx: Receiver<Result<AvatarImage, String>>,
    avatar_texture: Option<egui::TextureHandle>,
    update_rx: Option<Receiver<Result<Option<UpdateInfo>, String>>>,
    update_download_rx: Option<Receiver<Result<(), String>>>,
    update_info: Option<UpdateInfo>,
    update_checking: bool,
    update_downloading: bool,
    update_notice: Option<String>,
    restart_for_update: bool,
    page: Page,
    minecraft_tab: MinecraftTab,
    settings_tab: SettingsTab,
    launch_cancel: Option<Arc<AtomicBool>>,
    mods: Vec<ModProject>,
    mod_search: String,
    mod_category: String,
    mod_sort: String,
    mod_status: String,
    mod_searching: bool,
    mod_installing: bool,
    mod_search_rx: Option<Receiver<Result<Vec<ModProject>, String>>>,
    mod_install_rx: Option<Receiver<Result<String, String>>>,
    versions: Vec<VersionInfo>,
    latest_release: String,
    account: Option<Account>,
    auth_rx: Receiver<AuthEvent>,
    auth_tx: Sender<AuthEvent>,
    launch_rx: Option<Receiver<LaunchEvent>>,
    auth_busy: bool,
    launch_busy: bool,
    install_busy: bool,
    version_type_filter: String,
    status: String,
    error: Option<String>,
    device_code: Option<(String, String, String)>,
    version_search: String,
    version_rx: Option<Receiver<Result<(Vec<VersionInfo>, String), String>>>,
    directory_input: String,
    logs: Vec<String>,
    skin_pixels: Vec<u8>,
    skin_color: Color32,
    skin_texture: Option<egui::TextureHandle>,
    skin_status: String,
    skin_tool: SkinTool,
    skin_zoom: f32,
    skin_brush_size: u8,
    skin_mirror: bool,
    skin_region: String,
    skin_show_grid: bool,
    skin_show_overlay: bool,
    skin_alpha: u8,
    skin_cursor: Option<(usize, usize)>,
    close_after_launch: bool,
    skin_history_undo: Vec<Vec<u8>>,
    skin_history_redo: Vec<Vec<u8>>,
    boot_stage: BootStage,
    boot_stage_started: Instant,
    boot_java_rx: Option<Receiver<Option<String>>>,
    detected_java: Option<String>,
    logo_texture: egui::TextureHandle,
    page_transition_started: Instant,
    mod_issues: Vec<mods::ModIssue>,
    crash_report: Option<String>,
    benchmark_rx: Option<Receiver<system_info::BenchmarkResult>>,
    benchmark_result: Option<system_info::BenchmarkResult>,
    benchmark_busy: bool,
    ctx: egui::Context,
}

impl MintyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = config::load().unwrap_or_default();
        configure_style(&cc.egui_ctx, Color32::from_rgb(
            config.accent_color[0], config.accent_color[1], config.accent_color[2],
        ));
        let directory_input = config.game_dir.to_string_lossy().into_owned();
        let logs = config::recent_logs(300).unwrap_or_default();
        let (auth_tx, auth_rx) = unbounded();
        let (avatar_tx, avatar_rx) = unbounded();
        let hardware = system_info::detect();
        let icon = image::load_from_memory(include_bytes!("../assets/minty-launcher-icon.png"))
            .expect("Minty icon must be a valid PNG").to_rgba8();
        let (icon_w, icon_h) = icon.dimensions();
        let logo_texture = cc.egui_ctx.load_texture(
            "minty-logo",
            egui::ColorImage::from_rgba_unmultiplied([icon_w as usize, icon_h as usize], &icon.into_raw()),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            config,
            hardware,
            avatar_tx,
            avatar_rx,
            avatar_texture: None,
            update_rx: None,
            update_download_rx: None,
            update_info: None,
            update_checking: false,
            update_downloading: false,
            update_notice: None,
            restart_for_update: false,
            page: Page::Home,
            minecraft_tab: MinecraftTab::Builds,
            settings_tab: SettingsTab::General,
            launch_cancel: None,
            mods: Vec::new(),
            mod_search: String::new(),
            mod_category: "mod".to_owned(),
            mod_sort: "downloads".to_owned(),
            mod_status: "Каталог Modrinth: моды, сборки, ресурспаки и шейдеры.".to_owned(),
            mod_searching: false,
            mod_installing: false,
            mod_search_rx: None,
            mod_install_rx: None,
            versions: Vec::new(),
            latest_release: String::new(),
            account: None,
            auth_rx,
            auth_tx,
            launch_rx: None,
            auth_busy: false,
            launch_busy: false,
            install_busy: false,
            version_type_filter: "Все".to_owned(),
            status: "Minty Launcher готов к работе".to_owned(),
            error: None,
            device_code: None,
            version_search: String::new(),
            version_rx: None,
            directory_input,
            logs,
            skin_pixels: skin::blank(),
            skin_color: Color32::from_rgb(110, 190, 145),
            skin_texture: None,
            skin_status: "Импортируй PNG-скин или создай текстуру с нуля".to_owned(),
            skin_tool: SkinTool::Brush,
            skin_zoom: 7.0,
            skin_brush_size: 1,
            skin_mirror: false,
            skin_region: "Вся текстура".to_owned(),
            skin_show_grid: true,
            skin_show_overlay: true,
            skin_alpha: 255,
            skin_cursor: None,
            close_after_launch: false,
            skin_history_undo: Vec::new(),
            skin_history_redo: Vec::new(),
            boot_stage: BootStage::CheckingUpdates,
            boot_stage_started: Instant::now(),
            boot_java_rx: None,
            detected_java: None,
            logo_texture,
            page_transition_started: Instant::now(),
            mod_issues: Vec::new(),
            crash_report: None,
            benchmark_rx: None,
            benchmark_result: None,
            benchmark_busy: false,
            ctx: cc.egui_ctx.clone(),
        };
        if let Ok(data_dir) = config::data_dir() {
            let local_skin = data_dir.join("skin.png");
            if local_skin.is_file() {
                if let Ok(pixels) = skin::load_png(&local_skin) {
                    app.skin_pixels = pixels;
                    app.skin_status = "Загружен локальный скин Minty".to_owned();
                }
            }
        }
        if app.config.check_updates_on_start {
            app.check_updates();
        } else {
            app.begin_java_search();
        }
        app
    }

    fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = message.clone();
        let _ = config::append_log(&message);
        self.logs = config::recent_logs(300).unwrap_or_else(|_| self.logs.clone());
    }

    fn load_versions(&mut self) {
        self.log("Запрашиваю официальный список версий Mojang…");
        let (tx, rx) = unbounded::<Result<(Vec<VersionInfo>, String), String>>();
        self.version_rx = Some(rx);
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let result = minecraft::fetch_versions().map_err(|e| format!("{e:#}"));
            tx.send(result).ok();
            ctx.request_repaint();
        });
    }

    fn refresh_account(&mut self) {
        if self.config.microsoft_client_id.trim().is_empty() {
            return;
        }
        let client_id = self.config.microsoft_client_id.clone();
        let tx = self.auth_tx.clone();
        self.auth_busy = true;
        thread::spawn(move || match auth::refresh_saved_account(&client_id) {
            Ok(account) => {
                tx.send(AuthEvent::Connected(account)).ok();
            }
            Err(error) => {
                tx.send(AuthEvent::Failed(format!("Автовход: {error:#}"))).ok();
            }
        });
    }

    fn load_avatar(&mut self, uuid: String) {
        if uuid.trim().is_empty() { return; }
        let tx = self.avatar_tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<AvatarImage, String> {
                let url = format!("https://crafatar.imc.moe/avatars/{uuid}?size=128&overlay");
                let client = reqwest::blocking::Client::builder()
                    .user_agent(concat!("MintyLauncher/", env!("CARGO_PKG_VERSION")))
                    .timeout(Duration::from_secs(20))
                    .build().map_err(|error| error.to_string())?;
                let bytes = client.get(url).send()
                    .map_err(|error| format!("Аватар временно недоступен: {error}"))?
                    .error_for_status().map_err(|error| error.to_string())?
                    .bytes().map_err(|error| error.to_string())?;
                let image = image::load_from_memory(&bytes).map_err(|error| error.to_string())?.to_rgba8();
                let (width, height) = image.dimensions();
                Ok(AvatarImage { width: width as usize, height: height as usize, rgba: image.into_raw() })
            })();
            tx.send(result).ok();
            ctx.request_repaint();
        });
    }

    fn check_updates(&mut self) {
        if self.update_checking { return; }
        let (tx, rx) = unbounded();
        self.update_rx = Some(rx);
        self.update_checking = true;
        self.update_notice = Some("Проверяю обновления Minty Launcher…".to_owned());
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(updater::check_latest().map_err(|error| format!("{error:#}"))).ok();
            ctx.request_repaint();
        });
    }

    fn begin_java_search(&mut self) {
        if self.boot_stage != BootStage::CheckingUpdates { return; }
        self.boot_stage = BootStage::SearchingJava;
        self.boot_stage_started = Instant::now();
        let root = config::data_dir().map(|path| path.join("runtimes")).unwrap_or_default();
        let (tx, rx) = unbounded();
        self.boot_java_rx = Some(rx);
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(crate::runtime::find_installed_java(&root).map(|p| p.to_string_lossy().into_owned())).ok();
            ctx.request_repaint();
        });
    }

    fn start_update(&mut self) {
        if self.update_downloading { return; }
        let Some(info) = self.update_info.clone() else { return; };
        let (tx, rx) = unbounded();
        self.update_download_rx = Some(rx);
        self.update_downloading = true;
        self.update_notice = Some(format!("Загружаю обновление {}…", info.version));
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(updater::download_and_schedule(&info).map_err(|error| format!("{error:#}"))).ok();
            ctx.request_repaint();
        });
    }

    fn open_manual_update(&self) {
        let url = self.update_info.as_ref()
            .map(|info| info.release_url.as_str())
            .unwrap_or("https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases");
        let _ = open::that(url);
    }

    fn skip_update(&mut self) {
        if let Some(info) = &self.update_info {
            self.config.skipped_update_version = Some(info.version.clone());
            let _ = config::save(&self.config);
            self.log(format!("Обновление {} пропущено", info.version));
        }
        self.update_info = None;
        self.update_notice = Some("Эта версия обновления пропущена.".to_owned());
    }

    fn start_benchmark(&mut self) {
        if self.benchmark_busy { return; }
        let (tx, rx) = unbounded();
        self.benchmark_rx = Some(rx);
        self.benchmark_busy = true;
        self.benchmark_result = None;
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(system_info::run_benchmark()).ok();
            ctx.request_repaint();
        });
    }

    fn poll_background_tasks(&mut self) {
        let benchmark_result = self.benchmark_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = benchmark_result {
            self.benchmark_rx = None;
            self.benchmark_busy = false;
            self.benchmark_result = Some(result);
            self.log("Тест CPU и памяти завершён");
        }
        if let Ok(result) = self.avatar_rx.try_recv() {
            if let Ok(avatar) = result {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [avatar.width, avatar.height],
                    &avatar.rgba,
                );
                self.avatar_texture = Some(self.ctx.load_texture(
                    "profile-avatar",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }

        let update_result = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = update_result {
            self.update_rx = None;
            self.update_checking = false;
            match result {
                Ok(Some(info)) => {
                    let skipped = self.config.skipped_update_version.as_deref() == Some(info.version.as_str());
                    if skipped {
                        self.update_notice = Some(format!("Доступна пропущенная версия {}.", info.version));
                    } else {
                        self.update_notice = None;
                        self.update_info = Some(info);
                    }
                }
                Ok(None) => {
                    self.update_info = None;
                    self.update_notice = Some("Установлена последняя версия Minty Launcher.".to_owned());
                }
                Err(error) => {
                    self.update_info = None;
                    self.update_notice = Some(error);
                }
            }
        }

        if self.boot_stage == BootStage::CheckingUpdates
            && (!self.update_checking || self.boot_stage_started.elapsed() >= Duration::from_secs(8)) {
            self.begin_java_search();
        }
        let java_result = self.boot_java_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(path) = java_result {
            self.boot_java_rx = None;
            self.detected_java = path;
            self.log(if self.detected_java.is_some() {
                "Установленная Java найдена."
            } else {
                "Java не найдена. Подходящий Temurin будет установлен при первом запуске."
            });
            self.boot_stage = BootStage::Loading;
            self.boot_stage_started = Instant::now();
            self.load_versions();
            self.refresh_account();
        }
        if self.boot_stage == BootStage::Loading
            && ((self.version_rx.is_none() && self.boot_stage_started.elapsed() >= Duration::from_millis(700))
                || self.boot_stage_started.elapsed() >= Duration::from_secs(18)) {
            self.boot_stage = BootStage::Ready;
            self.page_transition_started = Instant::now();
            self.log("Minty Launcher готов к работе");
        }

        let download_result = self.update_download_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = download_result {
            self.update_download_rx = None;
            self.update_downloading = false;
            match result {
                Ok(()) => {
                    self.update_notice = Some("Обновление подготовлено. Minty Launcher перезапустится автоматически.".to_owned());
                    self.restart_for_update = true;
                }
                Err(error) => self.update_notice = Some(format!("Не удалось обновиться автоматически: {error}")),
            }
        }
    }

    fn cancel_launch(&mut self) {
        if let Some(cancel) = &self.launch_cancel {
            cancel.store(true, Ordering::Relaxed);
            self.status = "Отменяю подготовку Minecraft…".to_owned();
            self.log("Пользователь запросил отмену запуска Minecraft.");
        }
    }

    fn profile_instance_dir(&self) -> PathBuf {
        config::instance_dir(&self.config)
    }

    fn search_mod_store(&mut self) {
        if self.mod_searching || self.mod_installing { return; }
        let query = self.mod_search.trim().to_owned();
        let game_version = self.config.selected_version.clone();
        let loader = self.config.selected_loader.clone();
        let category = self.mod_category.clone();
        let sort = self.mod_sort.clone();
        let (tx, rx) = unbounded();
        self.mod_search_rx = Some(rx);
        self.mod_searching = true;
        self.mod_status = if query.is_empty() {
            "Загружаю популярные проекты каталога Modrinth…".to_owned()
        } else {
            "Ищу совместимые моды в каталоге Modrinth…".to_owned()
        };
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(mods::search(&query, &game_version, &loader, &category, &sort).map_err(|error| format!("{error:#}"))).ok();
            ctx.request_repaint();
        });
    }

    fn install_store_mod(&mut self, slug: String) {
        if self.mod_installing { return; }
        if self.mod_category != "mod" {
            self.mod_status = "Для этого типа проекта открой его страницу: модпаки, ресурспаки и шейдеры требуют разных форматов импорта.".to_owned();
            let _ = open::that(format!("https://modrinth.com/{}/{}", self.mod_category, slug));
            return;
        }
        if self.config.selected_loader == "Vanilla" {
            self.mod_status = "Для модов сначала выбери Fabric, Forge, NeoForge или Quilt.".to_owned();
            return;
        }
        let game_version = self.config.selected_version.clone();
        let loader = self.config.selected_loader.clone();
        let mods_dir = self.profile_instance_dir().join("mods");
        let (tx, rx) = unbounded();
        self.mod_install_rx = Some(rx);
        self.mod_installing = true;
        self.mod_status = format!("Устанавливаю {slug}…");
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(mods::install(&slug, &game_version, &loader, &mods_dir)
                .map(|path| format!("Мод установлен: {}", path.display()))
                .map_err(|error| format!("{error:#}"))).ok();
            ctx.request_repaint();
        });
    }

    fn import_game_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("Minecraft files", &["jar", "mcpack", "mcaddon", "mcworld"])
            .set_title("Импортировать мод или дополнение")
            .pick_files() else { return; };
        let mut imported = 0usize;
        let mut errors = Vec::new();
        for path in files {
            match path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
                "jar" => {
                    let mods_dir = self.profile_instance_dir().join("mods");
                    let result = (|| -> anyhow::Result<()> {
                        std::fs::create_dir_all(&mods_dir)?;
                        let filename = path.file_name().ok_or_else(|| anyhow::anyhow!("У файла нет имени"))?;
                        let mut destination = mods_dir.join(filename);
                        if destination.exists() {
                            let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("mod");
                            destination = mods_dir.join(format!("{stem}-imported.jar"));
                        }
                        std::fs::copy(&path, &destination)?;
                        Ok(())
                    })();
                    match result { Ok(()) => imported += 1, Err(error) => errors.push(format!("{}: {error}", path.display())) }
                }
                "mcpack" | "mcaddon" | "mcworld" => {
                    match bedrock::import_addon(&path) {
                        Ok(()) => imported += 1,
                        Err(error) => errors.push(format!("{}: {error:#}", path.display())),
                    }
                }
                _ => errors.push(format!("Неподдерживаемый файл: {}", path.display())),
            }
        }
        self.mod_status = if errors.is_empty() {
            format!("Передано в игру файлов: {imported}. Для Bedrock дождись сообщения об импорте внутри Minecraft.")
        } else {
            format!("Импортировано: {imported}. Ошибки: {}", errors.join(" | "))
        };
        self.log(self.mod_status.clone());
    }

    fn import_bedrock_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("Bedrock packages", &["mcpack", "mcaddon", "mcworld"])
            .set_title("Импортировать дополнение Bedrock")
            .pick_files() else { return; };
        let mut imported = 0usize;
        let mut errors = Vec::new();
        for path in files {
            match bedrock::import_addon(&path) {
                Ok(()) => imported += 1,
                Err(error) => errors.push(format!("{}: {error:#}", path.display())),
            }
        }
        self.mod_status = if errors.is_empty() {
            format!("Minecraft получил файлов для импорта: {imported}. Подтверждение появится в Bedrock.")
        } else {
            format!("Импортировано: {imported}. Ошибки: {}", errors.join(" | "))
        };
        self.log(self.mod_status.clone());
    }

    fn open_java_mods_folder(&self) {
        let path = self.profile_instance_dir().join("mods");
        let _ = std::fs::create_dir_all(&path);
        let _ = open::that(path);
    }

    fn poll_mod_tasks(&mut self) {
        let search_result = self.mod_search_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = search_result {
            self.mod_search_rx = None;
            self.mod_searching = false;
            match result {
                Ok(projects) => {
                    self.mod_status = format!("Найдено проектов: {}", projects.len());
                    self.mods = projects;
                }
                Err(error) => self.mod_status = format!("Поиск не выполнен: {error}"),
            }
        }
        let install_result = self.mod_install_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = install_result {
            self.mod_install_rx = None;
            self.mod_installing = false;
            match result {
                Ok(message) => {
                    self.mod_status = message.clone();
                    self.log(message);
                }
                Err(error) => self.mod_status = format!("Не удалось установить мод: {error}"),
            }
        }
    }

    fn begin_login(&mut self) {
        if self.config.microsoft_client_id.trim().is_empty() {
            self.error = Some(
                "Укажи Microsoft OAuth Client ID в настройках. Без него официальный вход недоступен."
                    .to_owned(),
            );
            self.switch_page(Page::Settings);
            return;
        }
        if self.auth_busy {
            return;
        }
        self.auth_busy = true;
        self.error = None;
        self.device_code = None;
        self.log("Запрашиваю код авторизации Microsoft…");
        let client_id = self.config.microsoft_client_id.clone();
        let tx = self.auth_tx.clone();
        thread::spawn(move || {
            if let Err(error) = auth::login_device(&client_id, tx.clone()) {
                tx.send(AuthEvent::Failed(format!("{error:#}"))).ok();
            }
        });
    }

    fn install_selected_version(&mut self, version_id: String) {
        if self.install_busy || self.launch_busy { return; }
        self.config.selected_version = version_id.clone();
        self.config.selected_profile_id.clear();
        if let Err(error) = config::save(&self.config) {
            self.error = Some(format!("Не удалось сохранить выбранную версию: {error:#}"));
            return;
        }
        let config = self.config.clone();
        let (tx, rx) = unbounded();
        self.launch_rx = Some(rx);
        self.install_busy = true;
        self.error = None;
        self.log(format!("Устанавливаю {} для Minecraft {version_id}…", self.config.selected_loader));
        thread::spawn(move || { let _ = minecraft::install_version(config, tx); });
    }

    fn start_game(&mut self) {
        if self.launch_busy || self.install_busy {
            return;
        }
        let selected_dir = PathBuf::from(self.directory_input.trim());
        if selected_dir.as_os_str().is_empty() || !selected_dir.is_absolute() {
            self.error = Some("Укажи абсолютный путь к папке Minecraft в настройках.".to_owned());
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&selected_dir) {
            self.error = Some(format!("Не удалось создать папку Minecraft: {error}"));
            return;
        }
        self.config.game_dir = selected_dir;
        if let Err(error) = config::save(&self.config) {
            self.error = Some(format!("Не удалось сохранить настройки: {error:#}"));
            return;
        }
        let account = self.account.clone().unwrap_or_else(|| Account {
            name: self.config.offline_username.trim().to_owned(),
            uuid: String::new(),
            xuid: String::new(),
            access_token: String::new(),
        });
        if account.name.is_empty() {
            self.error = Some("Введи ник для офлайн-режима или войди через Microsoft.".to_owned());
            return;
        }
        if !valid_username(&account.name) {
            self.error = Some(
                "Ник должен содержать от 3 до 16 латинских букв, цифр или подчёркиваний."
                    .to_owned(),
            );
            return;
        }
        self.error = None;
        self.launch_busy = true;
        self.log(format!("Подготовка Minecraft {}…", self.config.selected_version));
        let config = self.config.clone();
        let (tx, rx) = unbounded();
        self.launch_rx = Some(rx);
        let cancel = Arc::new(AtomicBool::new(false));
        self.launch_cancel = Some(cancel.clone());
        thread::spawn(move || {
            let _ = minecraft::launch(config, account, tx, cancel);
        });
    }

    fn apply_events(&mut self) {
        let auth_events: Vec<AuthEvent> = self.auth_rx.try_iter().collect();
        for event in auth_events {
            match event {
                AuthEvent::DeviceCode(info) => {
                    self.device_code = Some((
                        info.user_code,
                        info.verification_uri.clone(),
                        info.message,
                    ));
                    self.log("Ожидаю подтверждение входа на стороне Microsoft");
                    let _ = open::that(info.verification_uri);
                }
                AuthEvent::Connected(account) => {
                    self.config.last_account_name = Some(account.name.clone());
                    self.config.last_account_uuid = Some(account.uuid.clone());
                    self.config.last_account_xuid = Some(account.xuid.clone());
                    let _ = config::save(&self.config);
                    self.log(format!("Вход выполнен: {}", account.name));
                    self.load_avatar(account.uuid.clone());
                    self.account = Some(account);
                    self.auth_busy = false;
                    self.device_code = None;
                    self.error = None;
                }
                AuthEvent::Failed(message) => {
                    if !message.starts_with("Автовход:") {
                        self.error = Some(message.clone());
                    }
                    self.log(message);
                    self.auth_busy = false;
                    self.device_code = None;
                }
            }
        }

        let launch_events: Vec<LaunchEvent> = self
            .launch_rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in launch_events {
            match event {
                LaunchEvent::Status(message) => self.log(message),
                LaunchEvent::Installed(profile_id) => {
                    self.config.selected_profile_id = profile_id.clone();
                    let _ = config::save(&self.config);
                    self.install_busy = false;
                    self.log(format!("Установка завершена: {profile_id}"));
                }
                LaunchEvent::Finished(message) => {
                    self.log(message);
                    self.launch_busy = false;
                    self.install_busy = false;
                    self.launch_cancel = None;
                    if self.config.close_after_game {
                        self.close_after_launch = true;
                    }
                }
                LaunchEvent::Cancelled => {
                    self.log("Подготовка Minecraft отменена.");
                    self.launch_busy = false;
                    self.install_busy = false;
                    self.launch_cancel = None;
                }
                LaunchEvent::Failed(message) => {
                    self.error = Some(message.clone());
                    self.log("Не удалось запустить Minecraft");
                    self.launch_busy = false;
                    self.install_busy = false;
                    self.launch_cancel = None;
                }
            }
        }

        let version_result = self.version_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = version_result {
            self.version_rx = None;
            match result {
                Ok((versions, latest)) => {
                    self.versions = versions;
                    self.latest_release = latest.clone();
                    if self.config.selected_version == "latest-release" {
                        self.config.selected_version = latest;
                    }
                    if self
                        .versions
                        .iter()
                        .all(|v| v.id != self.config.selected_version)
                    {
                        self.config.selected_version = self.latest_release.clone();
                    }
                    let _ = config::save(&self.config);
                    self.log(format!(
                        "Список Minecraft загружен: {} официальных релизов",
                        self.versions.len()
                    ));
                }
                Err(error) => {
                    self.error = Some(format!("Не удалось получить версии: {error}"));
                    self.log("Нет подключения к серверам Mojang");
                }
            }
        }
        self.poll_background_tasks();
        self.poll_mod_tasks();
    }

    fn save_settings(&mut self) {
        let candidate = PathBuf::from(self.directory_input.trim());
        if candidate.as_os_str().is_empty() {
            self.error = Some("Укажи папку для файлов Minecraft.".to_owned());
            return;
        }
        if !candidate.is_absolute() {
            self.error = Some("Путь к папке Minecraft должен быть абсолютным.".to_owned());
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&candidate) {
            self.error = Some(format!("Не удалось создать папку Minecraft: {error}"));
            return;
        }
        self.config.game_dir = candidate;
        if self.config.auto_memory {
            self.config.max_memory_gb = self.hardware.recommended_memory_gb;
        }
        match config::save(&self.config) {
            Ok(()) => {
                self.error = None;
                self.log("Настройки сохранены");
            }
            Err(error) => self.error = Some(format!("Не удалось сохранить настройки: {error:#}")),
        }
    }

    fn choose_game_directory(&mut self) {
        let start = PathBuf::from(self.directory_input.trim());
        let mut dialog = rfd::FileDialog::new().set_title("Папка установки Minecraft");
        if start.is_dir() {
            dialog = dialog.set_directory(start);
        }
        if let Some(path) = dialog.pick_folder() {
            self.directory_input = path.to_string_lossy().into_owned();
            self.config.game_dir = path;
            self.log("Выбрана новая папка Minecraft. Нажми «Сохранить», чтобы применить путь.");
        }
    }

    fn import_skin(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Minecraft skin PNG", &["png"])
            .set_title("Импорт Minecraft skin")
            .pick_file()
        {
            match skin::load_png(&path) {
                Ok(pixels) => {
                    self.push_skin_undo();
                    self.skin_pixels = pixels;
                    self.skin_texture = None;
                    self.skin_status = format!("Загружен: {}", path.display());
                    let _ = config::append_log(&format!("Импортирован скин: {}", path.display()));
                }
                Err(error) => self.skin_status = format!("Ошибка импорта: {error:#}"),
            }
        }
    }

    fn export_skin(&mut self) {
        let suggested = format!("{}-skin.png", self.config.offline_username);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(&suggested)
            .set_title("Сохранить Minecraft skin")
            .save_file()
        {
            let path = if path.extension().is_none() {
                path.with_extension("png")
            } else {
                path
            };
            match skin::save_png(&path, &self.skin_pixels) {
                Ok(()) => {
                    self.skin_status = format!("Скин сохранён: {}", path.display());
                    let _ = config::append_log(&format!("Экспортирован скин: {}", path.display()));
                }
                Err(error) => self.skin_status = format!("Ошибка сохранения: {error:#}"),
            }
        }
    }

    fn save_local_skin(&mut self) {
        let result = config::data_dir()
            .map_err(anyhow::Error::from)
            .and_then(|dir| skin::save_png(&dir.join("skin.png"), &self.skin_pixels));
        match result {
            Ok(()) => self.skin_status = "Скин сохранён в локальный профиль Minty".to_owned(),
            Err(error) => self.skin_status = format!("Не удалось сохранить скин: {error:#}"),
        }
    }

    fn flip_skin_horizontal(&mut self) {
        self.push_skin_undo();
        let original = self.skin_pixels.clone();
        for y in 0..64 {
            for x in 0..64 {
                let source = (y * 64 + x) * 4;
                let target = (y * 64 + (63 - x)) * 4;
                self.skin_pixels[target..target + 4].copy_from_slice(&original[source..source + 4]);
            }
        }
        self.skin_texture = None;
        self.skin_status = "Текстура отражена по горизонтали".to_owned();
    }

    fn push_skin_undo(&mut self) {
        if self.skin_history_undo.len() >= 40 { self.skin_history_undo.remove(0); }
        self.skin_history_undo.push(self.skin_pixels.clone());
        self.skin_history_redo.clear();
    }

    fn undo_skin(&mut self) {
        if let Some(previous)=self.skin_history_undo.pop() {
            self.skin_history_redo.push(self.skin_pixels.clone());
            self.skin_pixels=previous; self.skin_texture=None;
            self.skin_status="Действие отменено".to_owned();
        }
    }

    fn redo_skin(&mut self) {
        if let Some(next)=self.skin_history_redo.pop() {
            self.skin_history_undo.push(self.skin_pixels.clone());
            self.skin_pixels=next; self.skin_texture=None;
            self.skin_status="Действие повторено".to_owned();
        }
    }

    fn paint_skin_pixel(&mut self,x:usize,y:usize,color:Color32) {
        let radius=(self.skin_brush_size.saturating_sub(1)/2) as isize;
        let mut centres=vec![(x,y)];
        if self.skin_mirror { centres.push((63-x,y)); }
        for (cx,cy) in centres {
            for dy in -radius..=radius { for dx in -radius..=radius {
                let px=cx as isize+dx; let py=cy as isize+dy;
                if !(0..64).contains(&px)||!(0..64).contains(&py) { continue; }
                let o=(py as usize*64+px as usize)*4;
                self.skin_pixels[o..o+4].copy_from_slice(&color.to_array());
            }}
        }
        self.skin_texture=None; self.skin_status=format!("Изменён пиксель: {x}, {y}");
    }

    fn fill_skin_area(&mut self,x:usize,y:usize,color:Color32) {
        let start=(y*64+x)*4;
        let target=[self.skin_pixels[start],self.skin_pixels[start+1],self.skin_pixels[start+2],self.skin_pixels[start+3]];
        if target==color.to_array(){return;}
        let mut stack=vec![(x,y)]; let mut visited=vec![false;4096];
        while let Some((px,py))=stack.pop(){
            let index=py*64+px;
            if visited[index]{continue;} visited[index]=true;
            let o=index*4;
            if self.skin_pixels[o..o+4]!=target{continue;}
            self.skin_pixels[o..o+4].copy_from_slice(&color.to_array());
            if px>0{stack.push((px-1,py));} if px<63{stack.push((px+1,py));}
            if py>0{stack.push((px,py-1));} if py<63{stack.push((px,py+1));}
        }
        self.skin_texture=None; self.skin_status="Заливка области завершена".to_owned();
    }

    fn accent_color(&self) -> Color32 {
        Color32::from_rgb(
            self.config.accent_color[0],
            self.config.accent_color[1],
            self.config.accent_color[2],
        )
    }

    fn avatar_widget(&self, ui: &mut egui::Ui, size: f32) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
        if let Some(texture) = &self.avatar_texture {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().circle_filled(rect.center(), size * 0.48, self.accent_color());
            let name = self.account.as_ref().map(|account| account.name.as_str())
                .unwrap_or(self.config.offline_username.as_str());
            let initial = name.chars().next().unwrap_or('M').to_uppercase().to_string();
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial,
                FontId::proportional(size * 0.42), Color32::from_rgb(16, 30, 27));
        }
        ui.painter().rect_stroke(
            rect.expand(1.0),
            Rounding::same(size * 0.22),
            Stroke::new(1.5, self.accent_color()),
        );
        response.on_hover_text("Открыть профиль и редактор скинов")
    }

    fn open_skin_workshop(&mut self) {
        self.switch_page(Page::Minecraft);
        self.minecraft_tab = MinecraftTab::Profile;
        self.page_transition_started = Instant::now();
        self.skin_status = "Редактор скинов открыт. Импортируй PNG или рисуй на UV-атласе.".to_owned();
    }

    fn window_titlebar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("minty_custom_titlebar")
            .exact_height(38.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(9, 18, 22))
                .stroke(Stroke::new(1.0, self.accent_color())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let width = (ui.available_width() - 142.0).max(180.0);
                    let (drag_rect, drag_response) = ui.allocate_exact_size(
                        egui::vec2(width, 34.0), egui::Sense::click_and_drag()
                    );
                    ui.painter().text(
                        drag_rect.left_center() + egui::vec2(2.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("MINTY  /  LAUNCHER  {}", env!("CARGO_PKG_VERSION")),
                        FontId::proportional(10.5),
                        self.accent_color(),
                    );
                    if drag_response.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag_response.double_clicked() {
                        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    let minimized = ui.add_sized(
                        [40.0, 29.0],
                        egui::Button::new(RichText::new("—").size(15.0).strong().color(TEXT))
                            .fill(PANEL).stroke(Stroke::new(1.0, self.accent_color()))
                            .rounding(Rounding::same(7.0)),
                    );
                    if minimized.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    let maximize = ui.add_sized(
                        [40.0, 29.0],
                        egui::Button::new(RichText::new(if maximized { "❐" } else { "□" }).size(13.0).color(TEXT))
                            .fill(PANEL).stroke(Stroke::new(1.0, self.accent_color()))
                            .rounding(Rounding::same(7.0)),
                    );
                    if maximize.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    let close = ui.add_sized(
                        [40.0, 29.0],
                        egui::Button::new(RichText::new("×").size(19.0).strong().color(TEXT))
                            .fill(Color32::from_rgb(65, 31, 39))
                            .stroke(Stroke::new(1.0, self.accent_color()))
                            .rounding(Rounding::same(7.0)),
                    );
                    if close.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.add_space(8.0);
                });
            });
    }

    fn ui_label(&self, key: &str) -> &'static str {
        translate_label(&self.config.language, key)
    }

    fn switch_page(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.page_transition_started = Instant::now();
        }
    }

    fn top_navigation(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("minty_top_navigation")
            .exact_height(68.0)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(22.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                    ui.painter().image(
                        self.logo_texture.id(),
                        logo,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.add_space(7.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("MINTY").size(18.0).strong().color(TEXT));
                        ui.label(RichText::new("JAVA  ·  BEDROCK").size(8.0).strong().color(MUTED));
                    });
                    ui.add_space(26.0);
                    self.top_tab(ui, self.ui_label("home"), Page::Home);
                    self.top_tab(ui, self.ui_label("minecraft"), Page::Minecraft);
                    self.top_tab(ui, self.ui_label("settings"), Page::Settings);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.launch_busy {
                            let cancel = egui::Button::new(RichText::new("Отмена запуска Minecraft").strong().color(Color32::WHITE))
                                .fill(RED).rounding(Rounding::same(9.0));
                            if ui.add_sized([164.0, 36.0], cancel).clicked() { self.cancel_launch(); }
                            ui.add_space(10.0);
                        }
                        let avatar_clicked = self.avatar_widget(ui, 38.0).clicked();
                        if avatar_clicked { self.open_skin_workshop(); }
                        ui.add_space(9.0);
                        ui.vertical(|ui| {
                            let name = self.account.as_ref().map(|account| account.name.as_str())
                                .unwrap_or(self.config.offline_username.as_str());
                            ui.add_sized([118.0, 18.0], egui::Label::new(RichText::new(name).strong().color(TEXT).size(11.0)).truncate());
                            ui.label(RichText::new(if self.account.is_some() { self.ui_label("microsoft") } else { self.ui_label("offline") })
                                .size(9.0).color(if self.account.is_some() { self.accent_color() } else { MUTED }));
                        });
                    });
                });
            });
    }

    fn top_tab(&mut self, ui: &mut egui::Ui, label: &str, page: Page) {
        let selected = self.page == page;
        let response = ui.add_sized(
            [104.0, 40.0],
            egui::Button::new(RichText::new(label).strong().size(12.5)
                .color(if selected { TEXT } else { MUTED }))
                .fill(if selected { PANEL_LIGHT } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                .rounding(Rounding::same(10.0)),
        );
        if response.clicked() { self.switch_page(page); }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    response.rect.left_bottom() - egui::vec2(0.0, 3.0),
                    egui::vec2(response.rect.width(), 3.0),
                ),
                Rounding::same(2.0),
                self.accent_color(),
            );
        }
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        let installed = scan_installed_profiles(&self.config.game_dir);
        let selected = if self.config.selected_profile_id.trim().is_empty() {
            self.config.selected_version.clone()
        } else { self.config.selected_profile_id.clone() };

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            egui::Frame::none().fill(Color32::from_rgb(23, 32, 35))
                .stroke(Stroke::new(1.0, Color32::from_rgb(48, 67, 65)))
                .rounding(Rounding::same(19.0)).inner_margin(egui::Margin::same(22.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MINTY LAUNCHER  /  JAVA + BEDROCK").size(9.0).strong().color(self.accent_color()));
                            ui.add_space(9.0);
                            ui.label(RichText::new("Твоя библиотека Minecraft.").size(29.0).strong().color(TEXT));
                            ui.add_space(5.0);
                            ui.label(RichText::new("Сборки, моды и игровые профили — в одном месте.").size(13.0).color(MUTED));
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("●").color(self.accent_color()).size(10.0));
                                ui.label(RichText::new("Локальные сборки").size(10.0).color(TEXT));
                                ui.add_space(10.0);
                                ui.label(RichText::new("●").color(self.accent_color()).size(10.0));
                                ui.label(RichText::new("Изоляция профилей").size(10.0).color(TEXT));
                            });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(egui::Image::new(&self.logo_texture).fit_to_exact_size(egui::vec2(104.0, 104.0)));
                        });
                    });
                });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("МОЯ БИБЛИОТЕКА").size(9.0).strong().color(self.accent_color()));
                    ui.label(RichText::new("Установленные сборки").size(22.0).strong().color(TEXT));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Открыть папку игры").clicked() {
                        let _ = std::fs::create_dir_all(&self.config.game_dir);
                        let _ = open::that(&self.config.game_dir);
                    }
                    if ui.button("Добавить версию").clicked() {
                        self.switch_page(Page::Minecraft);
                        self.minecraft_tab = MinecraftTab::Versions;
                    }
                });
            });
            ui.add_space(10.0);

            let mut selection = selected;
            let mut changed: Option<InstalledProfile> = None;
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ВЕРСИЯ ДЛЯ ЗАПУСКА").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("home_installed_profile_picker")
                                .selected_text(if installed.is_empty() { "Нет установленных сборок".to_owned() }
                                    else { installed.iter().find(|p| p.id == selection)
                                        .map(|p| format!("{}  ·  {}", p.id, p.game_version))
                                        .unwrap_or_else(|| "Выбери установленную сборку".to_owned()) })
                                .width(330.0).show_ui(ui, |ui| {
                                    for profile in &installed {
                                        let label = format!("{}  ·  {}  ·  {}", profile.id, profile.game_version, profile.loader);
                                        if ui.selectable_value(&mut selection, profile.id.clone(), label).changed() {
                                            changed = Some(profile.clone());
                                        }
                                    }
                                });
                            if let Some(profile) = installed.iter().find(|p| p.id == selection) {
                                ui.label(RichText::new(format!("Загрузчик: {}   ·   Minecraft {}", profile.loader, profile.game_version)).size(10.0).color(MUTED));
                            } else {
                                ui.label(RichText::new("Установи игру во вкладке «Minecraft → Версии».").size(10.0).color(MUTED));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let play = egui::Button::new(RichText::new(if self.launch_busy { "ПОДГОТОВКА…" } else { "▶  ИГРАТЬ" })
                                .size(14.0).strong().color(Color32::from_rgb(15, 35, 28)))
                                .fill(self.accent_color()).min_size(egui::vec2(178.0, 48.0)).rounding(Rounding::same(11.0));
                            if ui.add_enabled(!installed.is_empty() && !self.launch_busy && !self.install_busy, play).clicked() {
                                self.start_game();
                            }
                        });
                    });
                    if self.launch_busy {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Minecraft подготавливается. Загрузка файлов может занять несколько минут.").size(10.0).color(MUTED));
                        if ui.button("Отменить подготовку").clicked() { self.cancel_launch(); }
                    }
                });

            if let Some(profile) = changed {
                self.config.selected_version = profile.game_version.clone();
                self.config.selected_profile_id = if profile.id == profile.game_version { String::new() } else { profile.id.clone() };
                self.config.selected_loader = profile.loader;
                let _ = config::save(&self.config);
            }
            if installed.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("Здесь отображаются только сборки, файлы которых уже установлены. Полный каталог релизов находится отдельно во вкладке «Версии».").size(11.0).color(MUTED));
            }
            ui.add_space(14.0);
            ui.columns(3, |columns| {
                let cards = [
                    ("УСТАНОВЛЕНО", installed.len().to_string(), "Локальные игровые профили"),
                    ("ОЗУ", format!("{} ГБ", self.hardware.total_memory_gb), "Физическая память"),
                    ("ПОТОКИ CPU", self.hardware.logical_cores.to_string(), "Потоки процессора"),
                ];
                for (index, (title, value, hint)) in cards.iter().enumerate() {
                    egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0))
                        .inner_margin(egui::Margin::same(13.0)).show(&mut columns[index], |ui| {
                            ui.label(RichText::new(*title).size(9.0).strong().color(MUTED));
                            ui.label(RichText::new(value).size(25.0).strong().color(TEXT));
                            ui.label(RichText::new(*hint).size(9.0).color(MUTED));
                        });
                }
            });
            ui.add_space(12.0);
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ПРОИЗВОДИТЕЛЬНОСТЬ").size(9.0).strong().color(MUTED));
                            ui.label(RichText::new("Профиль «Слабый ПК»").strong().color(TEXT));
                            ui.label(RichText::new("Снижает дальность прорисовки, частицы и графические эффекты.").size(10.0).color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.checkbox(&mut self.config.weak_pc_mode, "Включить").changed() { let _ = config::save(&self.config); }
                        });
                    });
                });
        });
    }

    fn minecraft_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("MINECRAFT").size(9.0).strong().color(self.accent_color()));
                ui.label(RichText::new(self.ui_label("minecraft")).size(27.0).strong().color(TEXT));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Открыть папку игры").clicked() {
                    let _ = std::fs::create_dir_all(&self.config.game_dir);
                    let _ = open::that(&self.config.game_dir);
                }
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            self.minecraft_tab_button(ui, self.ui_label("builds"), MinecraftTab::Builds);
            self.minecraft_tab_button(ui, self.ui_label("versions"), MinecraftTab::Versions);
            self.minecraft_tab_button(ui, self.ui_label("profile"), MinecraftTab::Profile);
            self.minecraft_tab_button(ui, self.ui_label("mods"), MinecraftTab::Mods);
            self.minecraft_tab_button(ui, self.ui_label("bedrock"), MinecraftTab::Bedrock);
        });
        ui.add_space(10.0);
        match self.minecraft_tab {
            MinecraftTab::Builds => self.builds_page(ui),
            MinecraftTab::Versions => self.installations_page(ui),
            MinecraftTab::Profile => self.profile_page(ui),
            MinecraftTab::Mods => self.mods_page(ui),
            MinecraftTab::Bedrock => self.bedrock_page(ui),
        }
    }

    fn minecraft_tab_button(&mut self, ui: &mut egui::Ui, label: &str, tab: MinecraftTab) {
        let selected = self.minecraft_tab == tab;
        let response = ui.add_sized([108.0, 37.0],
            egui::Button::new(RichText::new(label).strong().size(11.5)
                .color(if selected { TEXT } else { MUTED }))
                .fill(if selected { PANEL_LIGHT } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                .rounding(Rounding::same(9.0)));
        if response.clicked() {
            self.minecraft_tab = tab;
            self.page_transition_started = Instant::now();
            if tab == MinecraftTab::Mods && self.mods.is_empty() && !self.mod_searching {
                self.mod_search.clear();
                self.search_mod_store();
            }
        }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(response.rect.left_bottom() - egui::vec2(0.0, 2.0),
                    egui::vec2(response.rect.width(), 2.0)),
                Rounding::same(1.0), self.accent_color());
        }
    }

    fn builds_page(&mut self, ui: &mut egui::Ui) {
        let versions_dir = self.config.game_dir.join("versions");
        let installed: Vec<String> = std::fs::read_dir(&versions_dir)
            .map(|entries| entries.flatten()
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect())
            .unwrap_or_default();
        ui.label(RichText::new("Текущая сборка").size(20.0).strong().color(TEXT));
        ui.label(RichText::new("Профиль, который будет запущен кнопкой «Играть».").size(11.0).color(MUTED));
        ui.add_space(12.0);
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&self.config.selected_version).size(23.0).strong().color(TEXT));
                        ui.label(RichText::new(format!("Minecraft Java · {}", self.config.selected_loader)).color(self.accent_color()));
                        ui.label(RichText::new(if self.config.selected_profile_id.is_empty() {
                            "Профиль ещё не установлен".to_owned()
                        } else {
                            format!("Профиль: {}", self.config.selected_profile_id)
                        }).size(10.0).color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Управлять версиями").clicked() { self.minecraft_tab = MinecraftTab::Versions; }
                        if ui.button("Открыть папку versions").clicked() { let _ = open::that(&versions_dir); }
                    });
                });
            });
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("УСТАНОВЛЕННЫЕ ВЕРСИИ").size(9.0).strong().color(MUTED));
            ui.label(RichText::new(format!("{} профилей", installed.len())).size(10.0).color(MUTED));
        });
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    if installed.is_empty() {
                        ui.label(RichText::new("Пока нет установленных профилей. Открой вкладку «Версии», чтобы установить Minecraft.")
                            .color(MUTED));
                    }
                    for profile in &installed {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("▦").color(self.accent_color()));
                            ui.label(RichText::new(profile).strong().color(TEXT));
                            if profile == &self.config.selected_version || profile == &self.config.selected_profile_id {
                                ui.label(RichText::new("ТЕКУЩИЙ").size(9.0).strong().color(self.accent_color()));
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Открыть").clicked() {
                                    let _ = open::that(versions_dir.join(profile));
                                }
                            });
                        });
                        ui.separator();
                    }
                });
            });
    }

    fn profile_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(5.0);
        ui.label(RichText::new(self.ui_label("profile")).size(23.0).strong().color(TEXT));
        ui.label(RichText::new("Аккаунт, аватар и внешний вид персонажа.").size(12.0).color(MUTED));
        ui.add_space(12.0);
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(15.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let _ = self.avatar_widget(ui, 64.0);
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        let name = self.account.as_ref().map(|account| account.name.as_str())
                            .unwrap_or(self.config.offline_username.as_str());
                        ui.label(RichText::new(name).size(18.0).strong().color(TEXT));
                        ui.label(RichText::new(if self.account.is_some() {
                            "Microsoft · Java Edition · лицензия подтверждена"
                        } else { "Офлайн-профиль · одиночная игра" })
                            .size(10.0).color(if self.account.is_some() { self.accent_color() } else { MUTED }));
                        if self.account.is_some() {
                            if ui.small_button("Выйти из аккаунта").clicked() {
                                auth::sign_out();
                                self.account = None;
                                self.avatar_texture = None;
                                self.config.last_account_name = None;
                                self.config.last_account_uuid = None;
                                self.config.last_account_xuid = None;
                                let _ = config::save(&self.config);
                                self.log("Выполнен выход из Microsoft-аккаунта");
                            }
                        } else if ui.small_button("Войти через Microsoft").clicked() {
                            self.begin_login();
                        }
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            if ui.button("Импортировать скин").clicked() { self.import_skin(); }
                            if ui.button("Сохранить локально").clicked() { self.save_local_skin(); }
                        });
                    });
                });
            });
        ui.add_space(13.0);
        self.skin_page(ui);
    }

    fn mods_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("Моды и дополнения").size(23.0).strong().color(TEXT));
            ui.label(RichText::new("Импортируй свои файлы или найди моды в каталоге Modrinth.")
                .size(12.0).color(MUTED));
            ui.add_space(12.0);
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ВЕРСИЯ").size(9.0).strong().color(MUTED));
                            ui.label(RichText::new(&self.config.selected_version).strong().color(TEXT));
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ЗАГРУЗЧИК").size(9.0).strong().color(MUTED));
                            ui.label(RichText::new(&self.config.selected_loader).strong().color(self.accent_color()));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Открыть папку mods").clicked() { self.open_java_mods_folder(); }
                            if ui.button("Импортировать файлы").clicked() { self.import_game_files(); }
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("КАТАЛОГ ПРОЕКТОВ").size(9.0).strong().color(MUTED));
                    let mut category_changed = false;
                    ui.horizontal_wrapped(|ui| {
                        for (label, category) in [
                            ("Моды", "mod"), ("Модпаки", "modpack"),
                            ("Ресурспаки", "resourcepack"), ("Шейдеры", "shader"),
                        ] {
                            let selected = self.mod_category == category;
                            if ui.add_sized([116.0, 34.0], egui::Button::new(RichText::new(label).strong().color(if selected { TEXT } else { MUTED }))
                                .fill(if selected { PANEL_LIGHT } else { BG }).rounding(Rounding::same(8.0))).clicked() {
                                self.mod_category = category.to_owned();
                                category_changed = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Сортировка").size(10.0).color(MUTED));
                        egui::ComboBox::from_id_source("mod_catalogue_sort")
                            .selected_text(match self.mod_sort.as_str() {
                                "follows" => "По подпискам",
                                "newest" => "Сначала новые",
                                "updated" => "Недавно обновлены",
                                "relevance" => "По релевантности",
                                _ => "По загрузкам",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.mod_sort, "downloads".to_owned(), "По загрузкам");
                                ui.selectable_value(&mut self.mod_sort, "follows".to_owned(), "По подпискам");
                                ui.selectable_value(&mut self.mod_sort, "newest".to_owned(), "Сначала новые");
                                ui.selectable_value(&mut self.mod_sort, "updated".to_owned(), "Недавно обновлены");
                                ui.selectable_value(&mut self.mod_sort, "relevance".to_owned(), "По релевантности");
                            });
                    });
                    if category_changed {
                        self.mod_search.clear();
                        self.search_mod_store();
                    }
                    ui.horizontal(|ui| {
                        let search_width = (ui.available_width() - 174.0).max(180.0);
                        ui.add_sized([search_width, 36.0], egui::TextEdit::singleline(&mut self.mod_search)
                            .hint_text("Найти мод: Sodium, JEI, Xaero's…"));
                        if ui.add_enabled(!self.mod_searching && !self.mod_installing,
                            egui::Button::new(if self.mod_searching { "Ищу…" } else { "Найти на Modrinth" })
                                .min_size(egui::vec2(160.0, 36.0))
                                .fill(self.accent_color()).rounding(Rounding::same(8.0))).clicked() {
                            self.search_mod_store();
                        }
                    });
                    ui.label(RichText::new("Для установки модов нужен Fabric, Forge, NeoForge или Quilt. Modrinth покажет совместимые версии.")
                        .size(10.0).color(MUTED));
                    ui.label(RichText::new(&self.mod_status).size(10.0).color(self.accent_color()));
                    ui.horizontal(|ui| {
                        if ui.button("Проверить конфликты").clicked() {
                            self.mod_issues = mods::analyze(&self.profile_instance_dir().join("mods"), &self.config.selected_loader);
                            self.mod_status = format!("Проверка завершена. Замечаний: {}", self.mod_issues.len());
                        }
                        if ui.button("Каталог CurseForge ↗").clicked() {
                            let _ = open::that("https://www.curseforge.com/minecraft/search?class=mc-mods");
                        }
                    });
                    for issue in &self.mod_issues {
                        let color = if issue.critical { RED } else { self.accent_color() };
                        ui.label(RichText::new(format!("{} {}", if issue.critical { "⚠" } else { "✓" }, issue.title)).size(10.0).strong().color(color));
                        ui.label(RichText::new(&issue.details).size(9.0).color(MUTED));
                    }
                });
            ui.add_space(12.0);
            let projects = self.mods.clone();
            let mut requested = None;
            if projects.is_empty() {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                    .inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(if self.mod_searching { "Ищем проекты в Modrinth…" } else { "Каталог пока пуст" })
                                .size(14.0).strong().color(TEXT));
                            ui.label(RichText::new("Попробуй изменить запрос или выбрать другую категорию.")
                                .size(10.0).color(MUTED));
                        });
                    });
            } else {
                ui.columns(2, |columns| {
                    for (index, project) in projects.iter().enumerate() {
                        let card_ui = &mut columns[index % 2];
                        egui::Frame::none()
                            .fill(PANEL)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(43, 57, 66)))
                            .rounding(Rounding::same(14.0))
                            .inner_margin(egui::Margin::same(12.0))
                            .show(card_ui, |ui| {
                                ui.set_min_height(178.0);
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, Rounding::same(11.0), PANEL_LIGHT);
                                    ui.painter().rect_stroke(rect, Rounding::same(11.0), Stroke::new(1.0, self.accent_color()));
                                    let mark = project.title.chars().next().unwrap_or('M').to_uppercase().to_string();
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, mark,
                                        FontId::proportional(20.0), self.accent_color());
                                    ui.vertical(|ui| {
                                        ui.add_sized([ui.available_width(), 19.0],
                                            egui::Label::new(RichText::new(&project.title).strong().color(TEXT).size(12.5)).truncate());
                                        ui.label(RichText::new(format!("{} загрузок", project.downloads))
                                            .size(9.0).color(self.accent_color()));
                                        if let Some(version) = &project.latest_version {
                                            ui.add_sized([ui.available_width(), 16.0],
                                                egui::Label::new(RichText::new(format!("Версия {version}")).size(9.0).color(MUTED)).truncate());
                                        }
                                    });
                                });
                                ui.add_space(6.0);
                                let description: String = project.description.chars().take(82).collect();
                                ui.add_sized([ui.available_width(), 38.0],
                                    egui::Label::new(RichText::new(description).size(9.5).color(MUTED)).wrap());
                                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                                    ui.horizontal(|ui| {
                                        let primary_label = if self.mod_category == "mod" { "Установить" } else { "Открыть проект" };
                                        let primary = ui.add_sized([112.0, 32.0],
                                            egui::Button::new(if self.mod_installing && self.mod_category == "mod" { "Установка…" } else { primary_label })
                                                .fill(if self.mod_category == "mod" { self.accent_color() } else { PANEL_LIGHT })
                                                .rounding(Rounding::same(8.0)));
                                        if primary.clicked() {
                                            if self.mod_category == "mod" { requested = Some(project.slug.clone()); }
                                            else { let _ = open::that(format!("https://modrinth.com/{}/{}", self.mod_category, project.slug)); }
                                        }
                                        if ui.add_sized([92.0, 32.0], egui::Button::new("Страница ↗")
                                            .fill(PANEL_LIGHT).rounding(Rounding::same(8.0))).clicked() {
                                            let _ = open::that(format!("https://modrinth.com/{}/{}", self.mod_category, project.slug));
                                        }
                                    });
                                });
                            });
                        columns[index % 2].add_space(8.0);
                    }
                });
                if let Some(slug) = requested { self.install_store_mod(slug); }
            }
            ui.add_space(13.0);
            let mods_dir = self.profile_instance_dir().join("mods");
            let installed_mods: Vec<PathBuf> = std::fs::read_dir(&mods_dir)
                .map(|entries| entries.flatten().map(|entry| entry.path())
                    .filter(|path| path.is_file() && path.extension().and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("jar")).unwrap_or(false))
                    .collect())
                .unwrap_or_default();
            ui.label(RichText::new(format!("УСТАНОВЛЕННЫЕ JAVA-МОДЫ · {}", installed_mods.len()))
                .size(9.0).strong().color(MUTED));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0))
                .inner_margin(egui::Margin::same(11.0)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(175.0).show(ui, |ui| {
                        if installed_mods.is_empty() {
                            ui.label(RichText::new("Папка mods пока пуста. Импортируй .jar или установи мод из каталога.")
                                .size(10.0).color(MUTED));
                        }
                        let mut remove_path = None;
                        for path in &installed_mods {
                            ui.horizontal(|ui| {
                                let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("mod.jar");
                                ui.label(RichText::new("◆").color(self.accent_color()));
                                ui.add_sized([260.0, 28.0], egui::Label::new(RichText::new(name).size(10.0).color(TEXT)).truncate());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_sized([88.0, 28.0], egui::Button::new("Удалить")
                                        .fill(Color32::from_rgb(62, 35, 42)).rounding(Rounding::same(7.0))).clicked() {
                                        remove_path = Some(path.clone());
                                    }
                                });
                            });
                            ui.separator();
                        }
                        if let Some(path) = remove_path {
                            match std::fs::remove_file(&path) {
                                Ok(()) => self.mod_status = format!("Мод удалён: {}", path.file_name().unwrap_or_default().to_string_lossy()),
                                Err(error) => self.mod_status = format!("Не удалось удалить мод: {error}"),
                            }
                        }
                    });
                });
            ui.add_space(8.0);
            ui.label(RichText::new("Встроенный каталог и поиск работают через Modrinth: моды устанавливаются напрямую, а сборки, ресурспаки и шейдеры открываются на странице проекта. CurseForge открывается официальным каталогом в браузере — его API требует отдельного ключа разработчика.")
                .size(9.0).color(MUTED));
        });
    }

    fn bedrock_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("Minecraft Bedrock").size(23.0).strong().color(TEXT));
            ui.label(RichText::new("Отдельный запуск Bedrock Edition и импорт дополнений.").size(12.0).color(MUTED));
            ui.add_space(12.0);
            egui::Frame::none().fill(Color32::from_rgb(29, 49, 45)).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(17.0)).stroke(Stroke::new(1.0, self.accent_color())).show(ui, |ui| {
                    ui.label(RichText::new("BEDROCK EDITION").size(9.0).strong().color(self.accent_color()));
                    ui.add_space(5.0);
                    ui.label(RichText::new("Minecraft для Windows").size(22.0).strong().color(TEXT));
                    ui.label(RichText::new(bedrock::installed_data_hint()).size(11.0).color(MUTED));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("КАНАЛ BEDROCK").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("bedrock_channel_picker")
                                .selected_text(&self.config.bedrock_channel).width(160.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.config.bedrock_channel, "Стабильная".to_owned(), "Стабильная");
                                    ui.selectable_value(&mut self.config.bedrock_channel, "Preview".to_owned(), "Preview");
                                });
                        });
                        if ui.add_sized([205.0, 43.0],
                            egui::Button::new(RichText::new("▶  Запустить выбранную").strong().color(Color32::from_rgb(15, 35, 28)))
                                .fill(self.accent_color()).rounding(Rounding::same(10.0))).clicked() {
                            let preview = self.config.bedrock_channel == "Preview";
                            match bedrock::launch(preview) {
                                Ok(()) => self.log(if preview { "Запускаю Minecraft Preview." } else { "Запускаю Minecraft Bedrock Stable." }),
                                Err(error) => self.error = Some(format!("{error:#}")),
                            }
                        }
                        if ui.button("Microsoft Store").clicked() {
                            if let Err(error) = bedrock::open_store_search() { self.error = Some(format!("{error:#}")); }
                        }
                        if ui.button("Импортировать дополнение").clicked() { self.import_bedrock_files(); }
                    });
                });
            ui.add_space(12.0);
            ui.label(RichText::new("НАЙДЕННЫЕ ДАННЫЕ BEDROCK").size(9.0).strong().color(MUTED));
            let installations = bedrock::installations();
            let has_preview = installations.iter().any(|item| item.channel == "Minecraft Preview");
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0))
                .inner_margin(egui::Margin::same(13.0)).show(ui, |ui| {
                    if installations.is_empty() {
                        ui.label(RichText::new("Папка com.mojang пока не найдена. Запусти Bedrock хотя бы один раз — затем здесь появятся папки дополнений и миров.")
                            .size(11.0).color(MUTED));
                    }
                    for installation in installations {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(installation.channel).strong().color(TEXT));
                                ui.label(RichText::new(installation.path.display().to_string()).size(9.0).color(MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Открыть папку").clicked() { let _ = open::that(&installation.path); }
                            });
                        });
                        ui.separator();
                    }
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Открыть папку данных Bedrock").clicked() {
                    if let Err(error) = bedrock::open_data_folder(false) { self.error = Some(format!("{error:#}")); }
                }
                if has_preview && ui.button("Открыть папку Preview").clicked() {
                    if let Err(error) = bedrock::open_data_folder(true) { self.error = Some(format!("{error:#}")); }
                }
            });
            ui.add_space(12.0);
            ui.label(RichText::new("Minty запускает установленные каналы Stable или Preview. Точную сборку и откат контролирует Microsoft Store; лаунчер не подменяет Appx-пакеты.")
                .size(10.0).color(MUTED));
            ui.label(RichText::new("Файлы .mcpack, .mcaddon и .mcworld открываются через зарегистрированный импорт Minecraft. После открытия дождись подтверждения импорта в самой игре.")
                .size(10.0).color(MUTED));
        });
    }

    fn installations_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(3.0);
        ui.label(RichText::new("Версии Java Edition").size(21.0).strong().color(TEXT));
        ui.label(RichText::new("Выбирай версию Minecraft и устанавливай совместимый загрузчик.")
            .size(11.0).color(MUTED));
        ui.add_space(12.0);
        let mut requested_install: Option<String> = None;
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(15.0)).show(ui, |ui| {
                let mut settings_changed = false;
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ЗАГРУЗЧИК").size(9.0).strong().color(MUTED));
                        egui::ComboBox::from_id_source("install_loader_picker")
                            .selected_text(&self.config.selected_loader).width(135.0)
                            .show_ui(ui, |ui| {
                                for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                    if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                        self.config.selected_profile_id.clear();
                                        self.config.selected_loader_version.clear();
                                        settings_changed = true;
                                    }
                                }
                            });
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ВЕРСИЯ ЗАГРУЗЧИКА").size(9.0).strong().color(MUTED));
                        if ui.add(egui::TextEdit::singleline(&mut self.config.selected_loader_version)
                            .desired_width(130.0).hint_text("Последняя стабильная")).changed() {
                            self.config.selected_profile_id.clear();
                            settings_changed = true;
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ПОИСК").size(9.0).strong().color(MUTED));
                        ui.add(egui::TextEdit::singleline(&mut self.version_search)
                            .desired_width(155.0).hint_text("1.21.4 / 24w14a"));
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ТИП").size(9.0).strong().color(MUTED));
                        egui::ComboBox::from_id_source("version_type_filter")
                            .selected_text(&self.version_type_filter).width(105.0)
                            .show_ui(ui, |ui| {
                                for kind in ["Все", "Release", "Snapshot", "Old Alpha", "Old Beta"] {
                                    ui.selectable_value(&mut self.version_type_filter, kind.to_owned(), kind);
                                }
                            });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("↻").on_hover_text("Обновить список версий").clicked() { self.load_versions(); }
                    });
                });
                if settings_changed { let _ = config::save(&self.config); }
                ui.add_space(8.0);
                ui.label(RichText::new("Оставь версию загрузчика пустой, чтобы выбрать последнюю совместимую стабильную сборку.")
                    .size(10.0).color(MUTED));
                ui.label(RichText::new(format!("Официальный последний релиз: {}", self.latest_release))
                    .size(10.0).color(MUTED));
                ui.add_space(8.0);
                let filtered: Vec<VersionInfo> = self.versions.iter().filter(|version| {
                    let query = self.version_search.is_empty()
                        || version.id.to_ascii_lowercase().contains(&self.version_search.to_ascii_lowercase());
                    let kind = match self.version_type_filter.as_str() {
                        "Release" => version.kind == "release",
                        "Snapshot" => version.kind == "snapshot",
                        "Old Alpha" => version.kind == "old_alpha",
                        "Old Beta" => version.kind == "old_beta",
                        _ => true,
                    };
                    query && kind
                }).cloned().collect();
                ui.label(RichText::new(format!("Найдено версий: {}", filtered.len())).size(10.0).color(MUTED));
                egui::ScrollArea::vertical().max_height(410.0).show(ui, |ui| {
                    for version in filtered {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&version.id).strong().color(TEXT).size(12.0));
                                let kind = match version.kind.as_str() {
                                    "release" => "СТАБИЛЬНАЯ",
                                    "snapshot" => "СНИМОК",
                                    "old_alpha" => "ALPHA",
                                    "old_beta" => "BETA",
                                    _ => "VERSION",
                                };
                                ui.label(RichText::new(kind).size(9.0).strong().color(self.accent_color()));
                            });
                            if let Some(date) = &version.time {
                                ui.label(RichText::new(date.get(..10).unwrap_or(date)).size(10.0).color(MUTED));
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let label = if self.config.selected_version == version.id { "Установить снова" } else { "Выбрать и скачать" };
                                if ui.add_enabled(!self.install_busy && !self.launch_busy,
                                    egui::Button::new(label).rounding(Rounding::same(8.0))).clicked() {
                                    requested_install = Some(version.id.clone());
                                }
                            });
                        });
                        ui.add_space(5.0);
                        ui.separator();
                    }
                    if self.versions.is_empty() {
                        ui.label(RichText::new("Список версий пока не загружен. Проверь интернет и нажми ↻.")
                            .color(MUTED));
                    }
                });
            });
        if let Some(version) = requested_install { self.install_selected_version(version); }
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Папка Java Edition: {}", self.config.game_dir.display())).size(10.0).color(MUTED));
    }

    fn skin_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("SKIN WORKSHOP  /  64×64").size(9.0).strong().color(self.accent_color()));
                    ui.label(RichText::new("Редактор скинов").size(27.0).strong().color(TEXT));
                    ui.label(RichText::new("Пиксельный UV-редактор с предпросмотром, слоями и историей.")
                        .size(11.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_sized([104.0, 34.0], egui::Button::new("Экспорт PNG")).clicked() { self.export_skin(); }
                    if ui.add_sized([104.0, 34.0], egui::Button::new("Импорт PNG")).clicked() { self.import_skin(); }
                });
            });
            ui.add_space(10.0);
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (label, action, enabled) in [
                            ("↶ Отменить", 0, !self.skin_history_undo.is_empty()),
                            ("↷ Повторить", 1, !self.skin_history_redo.is_empty()),
                            ("Отразить X", 2, true),
                            ("Сохранить в Minty", 3, true),
                            ("Очистить всё", 4, true),
                        ] {
                            if ui.add_enabled(enabled, egui::Button::new(label).min_size(egui::vec2(118.0, 32.0))
                                .fill(PANEL_LIGHT).rounding(Rounding::same(8.0))).clicked() {
                                match action {
                                    0 => self.undo_skin(),
                                    1 => self.redo_skin(),
                                    2 => self.flip_skin_horizontal(),
                                    3 => self.save_local_skin(),
                                    4 => {
                                        self.push_skin_undo();
                                        self.skin_pixels = skin::blank();
                                        self.skin_texture = None;
                                        self.skin_status = "Текстура полностью очищена".to_owned();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    });
                });
            ui.add_space(9.0);
            ui.horizontal_wrapped(|ui| {
                for label in ["Вся текстура", "Голова", "Тело", "Руки", "Ноги"] {
                    let selected = self.skin_region == label;
                    if ui.add_sized([112.0, 32.0],
                        egui::Button::new(RichText::new(label).strong().color(if selected { TEXT } else { MUTED }))
                            .fill(if selected { PANEL_LIGHT } else { BG }).rounding(Rounding::same(8.0))).clicked() {
                        self.skin_region = label.to_owned();
                    }
                }
            });
            ui.add_space(9.0);
            ui.columns(2, |columns| {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                    .inner_margin(egui::Margin::same(12.0)).show(&mut columns[0], |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("UV-АТЛАС").size(9.0).strong().color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.checkbox(&mut self.skin_show_grid, "Сетка");
                                ui.add(egui::Slider::new(&mut self.skin_zoom, 3.0..=5.5).show_value(false).text("Масштаб"));
                            });
                        });
                        self.skin_canvas(ui);
                        ui.horizontal(|ui| {
                            let cursor = self.skin_cursor.map(|(x, y)| format!("X: {x}  ·  Y: {y}"))
                                .unwrap_or_else(|| "Наведи курсор на пиксель".to_owned());
                            ui.label(RichText::new(cursor).monospace().size(9.0).color(self.accent_color()));
                        });
                        ui.label(RichText::new("Левая кнопка мыши рисует. Крупная сетка отмечает блоки 8×8.")
                            .size(9.0).color(MUTED));
                    });
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                    .inner_margin(egui::Margin::same(12.0)).show(&mut columns[1], |ui| {
                        ui.label(RichText::new("ИНСТРУМЕНТЫ").size(9.0).strong().color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for (name, tool) in [("✎ Кисть", SkinTool::Brush), ("◩ Ластик", SkinTool::Eraser),
                                ("▨ Заливка", SkinTool::Fill), ("⌖ Пипетка", SkinTool::Eyedropper)] {
                                let selected = self.skin_tool == tool;
                                if ui.add_sized([103.0, 32.0],
                                    egui::Button::new(RichText::new(name).strong().color(if selected { TEXT } else { MUTED }))
                                        .fill(if selected { PANEL_LIGHT } else { BG }).rounding(Rounding::same(8.0))).clicked() {
                                    self.skin_tool = tool;
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new("ЦВЕТ И КИСТЬ").size(9.0).strong().color(MUTED));
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut self.skin_color);
                            ui.label(RichText::new(format!("#{:02X}{:02X}{:02X}", self.skin_color.r(), self.skin_color.g(), self.skin_color.b()))
                                .monospace().size(10.0).color(TEXT));
                        });
                        ui.add(egui::Slider::new(&mut self.skin_brush_size, 1..=8).suffix(" px").text("Размер кисти"));
                        ui.add(egui::Slider::new(&mut self.skin_alpha, 0..=255).text("Непрозрачность"));
                        ui.checkbox(&mut self.skin_mirror, "Зеркальное рисование");
                        ui.checkbox(&mut self.skin_show_overlay, "Показывать внешний слой в превью");
                        ui.add_space(5.0);
                        ui.label(RichText::new("БЫСТРАЯ ПАЛИТРА").size(9.0).strong().color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for color in [
                                Color32::from_rgb(32, 36, 45), Color32::from_rgb(238, 193, 150),
                                Color32::from_rgb(76, 125, 204), Color32::from_rgb(184, 67, 79),
                                Color32::from_rgb(84, 166, 109), Color32::from_rgb(244, 211, 93),
                                Color32::from_rgb(39, 160, 144), Color32::from_rgb(160, 115, 205),
                                Color32::WHITE, Color32::BLACK, Color32::TRANSPARENT,
                            ] {
                                if ui.add_sized([25.0, 25.0], egui::Button::new(" ").fill(color)
                                    .stroke(Stroke::new(1.0, Color32::from_white_alpha(35)))).clicked() {
                                    self.skin_color = color;
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new("ПРЕДПРОСМОТР ПЕРСОНАЖА").size(9.0).strong().color(MUTED));
                        self.skin_preview(ui);
                        ui.label(RichText::new(&self.skin_status).size(10.0).color(self.accent_color()));
                    });
            });
            ui.label(RichText::new("Сохранение в Minty хранит PNG локально. Публикация на Microsoft/Mojang или Ely.by требует отдельной авторизации.")
                .size(9.0).color(MUTED));
        });
    }

    fn skin_canvas(&mut self,ui:&mut egui::Ui){
        let image=egui::ColorImage::from_rgba_unmultiplied([64,64],&self.skin_pixels);
        if let Some(texture)=self.skin_texture.as_mut(){texture.set(image,egui::TextureOptions::NEAREST);}
        else{self.skin_texture=Some(ui.ctx().load_texture("skin-editor",image,egui::TextureOptions::NEAREST));}
        let side=64.0*self.skin_zoom;
        let (rect,response)=ui.allocate_exact_size(egui::vec2(side,side),egui::Sense::click_and_drag());
        let painter=ui.painter(); let tile=side/8.0;
        for y in 0..8{for x in 0..8{
            let c=if (x+y)%2==0{PANEL_LIGHT}else{Color32::from_rgb(28,37,45)};
            painter.rect_filled(egui::Rect::from_min_size(rect.min+egui::vec2(x as f32*tile,y as f32*tile),egui::vec2(tile,tile)),Rounding::same(0.0),c);
        }}
        if let Some(texture)=self.skin_texture.as_ref(){
            painter.image(texture.id(),rect,egui::Rect::from_min_max(egui::pos2(0.0,0.0),egui::pos2(1.0,1.0)),Color32::WHITE);
        }
        if self.skin_show_grid {
            for i in 0..=64 {
                let x = rect.left() + rect.width() * i as f32 / 64.0;
                let y = rect.top() + rect.height() * i as f32 / 64.0;
                let major = i % 8 == 0;
                let c = if major { Color32::from_white_alpha(90) } else { Color32::from_white_alpha(25) };
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(if major { 1.0 } else { 0.5 }, c));
                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    Stroke::new(if major { 1.0 } else { 0.5 }, c));
            }
        }
        for (x,y,w,h) in skin_region_rects(&self.skin_region){
            let area=egui::Rect::from_min_max(egui::pos2(rect.left()+x as f32*side/64.0,rect.top()+y as f32*side/64.0),
                egui::pos2(rect.left()+(x+w) as f32*side/64.0,rect.top()+(y+h) as f32*side/64.0));
            painter.rect_stroke(area,Rounding::same(0.0),Stroke::new(1.5,self.accent_color()));
        }
        if response.hovered(){
            if let Some(pos)=response.interact_pointer_pos(){
                let x=(((pos.x-rect.left())/side)*64.0).floor().clamp(0.0,63.0) as usize;
                let y=(((pos.y-rect.top())/side)*64.0).floor().clamp(0.0,63.0) as usize;
                self.skin_cursor = Some((x, y));
                if response.drag_started()&&self.skin_tool!=SkinTool::Eyedropper{self.push_skin_undo();}
                if response.dragged()||response.clicked(){
                    match self.skin_tool{
                        SkinTool::Eyedropper=>{let o=(y*64+x)*4;self.skin_color=Color32::from_rgba_unmultiplied(
                            self.skin_pixels[o],self.skin_pixels[o+1],self.skin_pixels[o+2],self.skin_pixels[o+3]);}
                        SkinTool::Fill if response.clicked()=>{self.push_skin_undo();self.fill_skin_area(x,y,self.skin_color);}
                        SkinTool::Brush|SkinTool::Eraser=>{
                            if response.clicked()&&!response.dragged(){self.push_skin_undo();}
                            let c = if self.skin_tool == SkinTool::Eraser {
                                Color32::TRANSPARENT
                            } else {
                                Color32::from_rgba_unmultiplied(
                                    self.skin_color.r(), self.skin_color.g(), self.skin_color.b(), self.skin_alpha
                                )
                            };
                            self.paint_skin_pixel(x,y,c);
                        }
                        _=>{}
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    fn skin_preview(&mut self,ui:&mut egui::Ui){
        if self.skin_texture.is_none(){
            let image=egui::ColorImage::from_rgba_unmultiplied([64,64],&self.skin_pixels);
            self.skin_texture=Some(ui.ctx().load_texture("skin-editor-preview",image,egui::TextureOptions::NEAREST));
        }
        if let Some(texture)=self.skin_texture.as_ref(){
            let (rect,_)=ui.allocate_exact_size(egui::vec2(190.0,245.0),egui::Sense::hover());
            let painter=ui.painter();painter.rect_filled(rect,Rounding::same(8.0),PANEL_LIGHT);
            let uv=|x:u32,y:u32,w:u32,h:u32|egui::Rect::from_min_max(egui::pos2(x as f32/64.0,y as f32/64.0),
                egui::pos2((x+w) as f32/64.0,(y+h) as f32/64.0));
            let draw=|dest:egui::Rect,src:egui::Rect|painter.image(texture.id(),dest,src,Color32::WHITE);
            let x=rect.left();let y=rect.top();
            draw(egui::Rect::from_min_size(egui::pos2(x+66.0,y+7.0),egui::vec2(58.0,58.0)),uv(8,8,8,8));
            draw(egui::Rect::from_min_size(egui::pos2(x+57.0,y+69.0),egui::vec2(76.0,108.0)),uv(20,20,8,12));
            draw(egui::Rect::from_min_size(egui::pos2(x+31.0,y+69.0),egui::vec2(23.0,108.0)),uv(44,20,4,12));
            draw(egui::Rect::from_min_size(egui::pos2(x+136.0,y+69.0),egui::vec2(23.0,108.0)),uv(36,52,4,12));
            draw(egui::Rect::from_min_size(egui::pos2(x+58.0,y+180.0),egui::vec2(34.0,57.0)),uv(4,20,4,12));
            draw(egui::Rect::from_min_size(egui::pos2(x+98.0,y+180.0),egui::vec2(34.0,57.0)),uv(20,52,4,12));
            if self.skin_show_overlay {
                draw(egui::Rect::from_min_size(egui::pos2(x+66.0,y+7.0),egui::vec2(58.0,58.0)),uv(40,8,8,8));
                draw(egui::Rect::from_min_size(egui::pos2(x+57.0,y+69.0),egui::vec2(76.0,108.0)),uv(20,36,8,12));
                draw(egui::Rect::from_min_size(egui::pos2(x+31.0,y+69.0),egui::vec2(23.0,108.0)),uv(44,36,4,12));
            }
        }
    }

    fn journal_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(7.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Журнал лаунчера").size(27.0).strong().color(TEXT));
                ui.label(RichText::new("События авторизации, загрузки и запуска Minecraft.").size(13.0).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Очистить").clicked() {
                    match config::clear_logs() {
                        Ok(()) => {
                            self.logs.clear();
                            self.log("Журнал очищен");
                        }
                        Err(error) => self.error = Some(format!("Не удалось очистить журнал: {error}")),
                    }
                }
                if ui.button("Копировать").clicked() {
                    ui.output_mut(|output| output.copied_text = self.logs.join("\n"));
                }
                if ui.button("Обновить").clicked() {
                    self.logs = config::recent_logs(300).unwrap_or_default();
                }
            });
        });
        ui.add_space(10.0);
        if ui.button("Анализировать журнал Minecraft").clicked() {
            self.crash_report = Some(crash::analyze_latest().unwrap_or_else(|error| format!("Не удалось разобрать журнал: {error:#}")));
        }
        if let Some(report) = &self.crash_report {
            egui::Frame::none().fill(PANEL_LIGHT).rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    ui.label(RichText::new("ОТЧЁТ CRASH REPORTER").size(9.0).strong().color(self.accent_color()));
                    ui.label(RichText::new(report).size(10.0).color(TEXT));
                    if ui.small_button("Копировать отчёт").clicked() { ui.output_mut(|output| output.copied_text = report.clone()); }
                });
        }
        ui.add_space(10.0);
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0)).inner_margin(egui::Margin::same(13.0)).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(490.0).stick_to_bottom(true).show(ui, |ui| {
                if self.logs.is_empty() {
                    ui.label(RichText::new("Записей пока нет. Запусти Minecraft или измени настройки.").color(MUTED));
                }
                for line in &self.logs {
                    ui.label(RichText::new(line).monospace().size(10.0).color(MUTED));
                }
            });
        });
    }

    fn settings_hub(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Настройки лаунчера").size(25.0).strong().color(TEXT));
        ui.label(RichText::new("Разделы настроек Minty Launcher").size(11.0).color(MUTED));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::same(7.0)).show(ui, |ui| {
                    ui.set_min_width(168.0);
                    egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            self.settings_tab_button(ui, self.ui_label("general"), SettingsTab::General);
                            self.settings_tab_button(ui, self.ui_label("game"), SettingsTab::Game);
                            self.settings_tab_button(ui, self.ui_label("java"), SettingsTab::Java);
                            self.settings_tab_button(ui, self.ui_label("performance"), SettingsTab::Performance);
                            self.settings_tab_button(ui, self.ui_label("appearance"), SettingsTab::Appearance);
                            self.settings_tab_button(ui, self.ui_label("account"), SettingsTab::Account);
                            self.settings_tab_button(ui, self.ui_label("storage"), SettingsTab::Storage);
                            self.settings_tab_button(ui, self.ui_label("security"), SettingsTab::Security);
                            self.settings_tab_button(ui, self.ui_label("updates"), SettingsTab::Updates);
                            self.settings_tab_button(ui, self.ui_label("journal"), SettingsTab::Journal);
                        });
                    });
                });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                match self.settings_tab {
                    SettingsTab::General => self.settings_general_page(ui),
                    SettingsTab::Game => self.settings_game_page(ui),
                    SettingsTab::Java => self.settings_java_page(ui),
                    SettingsTab::Performance => self.settings_performance_page(ui),
                    SettingsTab::Appearance => self.settings_appearance_page(ui),
                    SettingsTab::Account => self.settings_account_page(ui),
                    SettingsTab::Storage => self.settings_storage_page(ui),
                    SettingsTab::Security => self.settings_security_page(ui),
                    SettingsTab::Updates => self.settings_updates_page(ui),
                    SettingsTab::Journal => self.journal_page(ui),
                }
            });
        });
    }

    fn settings_tab_button(&mut self, ui: &mut egui::Ui, label: &str, tab: SettingsTab) {
        let selected = self.settings_tab == tab;
        let response = ui.add_sized([152.0, 33.0],
            egui::Button::new(RichText::new(label).strong().size(10.5)
                .color(if selected { TEXT } else { MUTED }))
                .fill(if selected { PANEL_LIGHT } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                .rounding(Rounding::same(8.0)));
        if response.clicked() {
            self.settings_tab = tab;
            self.page_transition_started = Instant::now();
        }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(response.rect.left_bottom() - egui::vec2(0.0, 2.0),
                    egui::vec2(response.rect.width(), 2.0)),
                Rounding::same(1.0), self.accent_color());
        }
    }

    fn settings_save_button(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        if ui.add_sized([185.0, 39.0],
            egui::Button::new(RichText::new("СОХРАНИТЬ").strong().color(Color32::from_rgb(15, 35, 28)))
                .fill(self.accent_color()).rounding(Rounding::same(9.0))).clicked() {
            if self.settings_tab == SettingsTab::Game {
                self.save_settings();
            } else if let Err(error) = config::save(&self.config) {
                self.error = Some(format!("Не удалось сохранить: {error:#}"));
            } else {
                self.log("Настройки сохранены");
            }
        }
    }

    fn settings_general_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Основные").size(21.0).strong().color(TEXT));
            ui.label(RichText::new("Имя игрока и общие параметры интерфейса.").size(11.0).color(MUTED));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new("ОФЛАЙН-ИМЯ").size(9.0).strong().color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.config.offline_username).desired_width(330.0));
                    ui.label(RichText::new("Используется при запуске офлайн-профиля.").size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("ЯЗЫК МЕНЮ").size(9.0).strong().color(MUTED));
                    egui::ComboBox::from_id_source("minty_language_picker")
                        .selected_text(&self.config.language)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for language in ["Русский", "English", "Deutsch", "Français", "Español",
                                "Polski", "Українська", "Türkçe", "Português", "日本語"] {
                                ui.selectable_value(&mut self.config.language, language.to_owned(), language);
                            }
                        });
                    ui.label(RichText::new("Переведены навигация, названия разделов и ключевые элементы. Подробные технические описания пока остаются на русском.")
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.checkbox(&mut self.config.check_updates_on_start, "Проверять обновления при запуске");
                    ui.checkbox(&mut self.config.close_after_game, "Закрывать лаунчер после запуска Minecraft");
                    ui.checkbox(&mut self.config.enable_animations, "Анимировать переходы между разделами");
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_game_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Minecraft").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new("ПАПКА JAVA EDITION").size(9.0).strong().color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.directory_input).desired_width(340.0));
                        if ui.button("Обзор…").clicked() { self.choose_game_directory(); }
                        if ui.button("Открыть").clicked() { let _ = open::that(self.directory_input.trim()); }
                    });
                    ui.checkbox(&mut self.config.launch_fullscreen, "Запускать Java Edition в полноэкранном режиме");
                    ui.checkbox(&mut self.config.isolated_profiles, "Изолировать миры, моды и настройки для каждой сборки");
                    ui.label(RichText::new("При первом запуске в профиль копируются прежние worlds/mods/options. Исходные файлы не удаляются.")
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("BEDROCK").size(9.0).strong().color(MUTED));
                    egui::ComboBox::from_id_source("settings_bedrock_channel")
                        .selected_text(&self.config.bedrock_channel).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.bedrock_channel, "Стабильная".to_owned(), "Стабильная");
                            ui.selectable_value(&mut self.config.bedrock_channel, "Preview".to_owned(), "Preview");
                        });
                    ui.label(RichText::new("Номер сборки и откат Bedrock контролируются Microsoft Store.")
                        .size(10.0).color(MUTED));
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_java_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Java Runtime").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new("ПУТЬ К JAVA").size(9.0).strong().color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.config.java_path).desired_width(420.0)
                        .hint_text("Пусто — автоматический подбор"));
                    ui.label(RichText::new(self.detected_java.as_ref().map(|p| format!("Найдена: {p}"))
                        .unwrap_or_else(|| "Java будет найдена или установлена при запуске игры.".to_owned()))
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("СБОРЩИК МУСОРА").size(9.0).strong().color(MUTED));
                    egui::ComboBox::from_id_source("settings_gc")
                        .selected_text(&self.config.gc_algorithm).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.gc_algorithm, "Авто".to_owned(), "Авто — рекомендуется");
                            ui.selectable_value(&mut self.config.gc_algorithm, "G1GC".to_owned(), "G1GC");
                            ui.selectable_value(&mut self.config.gc_algorithm, "ZGC".to_owned(), "ZGC");
                            ui.selectable_value(&mut self.config.gc_algorithm, "Shenandoah".to_owned(), "Shenandoah");
                        });
                    ui.label(RichText::new("Автоподбор выбирает совместимую Java. Игра запускается через javaw.exe, а диагностические проверки используют скрытый режим.")
                        .size(10.0).color(MUTED));
                    ui.label(RichText::new("ZGC и Shenandoah требуют совместимой версии Java.")
                        .size(10.0).color(MUTED));
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_performance_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Производительность").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new(&self.hardware.cpu_name).strong().color(TEXT));
                    ui.label(RichText::new(format!("{} ГБ ОЗУ · {} потоков CPU", self.hardware.total_memory_gb, self.hardware.logical_cores))
                        .size(11.0).color(MUTED));
                    if ui.checkbox(&mut self.config.auto_memory, "Автоматически подбирать объём ОЗУ").changed()
                        && self.config.auto_memory {
                        self.config.max_memory_gb = self.hardware.recommended_memory_gb;
                    }
                    if self.config.auto_memory {
                        ui.label(RichText::new(format!("Рекомендуется {} ГБ", self.hardware.recommended_memory_gb))
                            .size(10.0).color(self.accent_color()));
                    } else {
                        let max = self.hardware.total_memory_gb.saturating_sub(2).min(16).max(2) as u8;
                        self.config.max_memory_gb = self.config.max_memory_gb.clamp(2, max);
                        ui.add(egui::Slider::new(&mut self.config.max_memory_gb, 2..=max).suffix(" ГБ"));
                    }
                    ui.separator();
                    if ui.add_enabled(!self.benchmark_busy, egui::Button::new(if self.benchmark_busy { "Тест выполняется…" } else { "Запустить тест CPU и памяти" })
                        .fill(self.accent_color()).rounding(Rounding::same(8.0))).clicked() {
                        self.start_benchmark();
                    }
                    if let Some(result) = &self.benchmark_result {
                        ui.label(RichText::new(format!("CPU: {} млн. операций/с · {} потоков", result.cpu_operations_per_second / 1_000_000, result.logical_threads))
                            .strong().color(self.accent_color()));
                        ui.label(RichText::new(format!("Память: {} МБ/с · время теста: {} мс", result.memory_mb_per_second, result.elapsed_ms))
                            .size(10.0).color(MUTED));
                        ui.label(RichText::new("Это синтетический тест CPU/памяти, а не прогноз FPS Minecraft.")
                            .size(9.0).color(MUTED));
                    }
                    ui.checkbox(&mut self.config.auto_graphics_profile, "Автоматически определять профиль графики");
                    ui.checkbox(&mut self.config.weak_pc_mode, "Режим «Слабый ПК»");
                    ui.label(RichText::new("Параметры оптимизации применяются к активному игровому профилю перед запуском.")
                        .size(10.0).color(MUTED));
                    ui.label(RichText::new("Режим оптимизирует поддерживаемые параметры options.txt. FPS также зависит от видеокарты, драйвера, мира и модов.")
                        .size(10.0).color(MUTED));
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_appearance_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Оформление").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new("АКЦЕНТНЫЙ ЦВЕТ").size(9.0).strong().color(MUTED));
                    ui.horizontal_wrapped(|ui| {
                        let mut accent = self.accent_color();
                        if ui.color_edit_button_srgba(&mut accent).changed() {
                            self.config.accent_color = [accent.r(), accent.g(), accent.b()];
                            configure_style(ui.ctx(), accent);
                        }
                        for (name, rgb) in [
                            ("Minty", [145, 226, 183]), ("Океан", [104, 190, 219]),
                            ("Лаванда", [174, 145, 224]), ("Янтарь", [232, 184, 104]),
                            ("Рубин", [220, 105, 115]),
                        ] {
                            if ui.button(name).clicked() {
                                self.config.accent_color = rgb;
                                configure_style(ui.ctx(), self.accent_color());
                            }
                        }
                    });
                    ui.checkbox(&mut self.config.enable_animations, "Плавные анимации вкладок");
                    ui.label(RichText::new("Кнопки свернуть, развернуть и закрыть используют собственную рамку Minty с акцентным цветом.")
                        .size(10.0).color(MUTED));
                    ui.label(RichText::new("Выбранный цвет применяется к рамке окна, активным вкладкам, выделению и основным кнопкам.")
                        .size(10.0).color(MUTED));
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_account_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Аккаунт").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    if let Some(account) = self.account.clone() {
                        ui.label(RichText::new(format!("Выполнен вход: {}", account.name)).strong().color(TEXT));
                        if ui.button("Выйти").clicked() {
                            auth::sign_out(); self.account = None; self.avatar_texture = None;
                            self.config.last_account_name = None; self.config.last_account_uuid = None;
                            self.config.last_account_xuid = None;
                        }
                    } else if ui.add_enabled(!self.auth_busy, egui::Button::new("Войти через Microsoft")
                        .fill(self.accent_color())).clicked() {
                        self.begin_login();
                    }
                    ui.separator();
                    ui.label(RichText::new("MICROSOFT PUBLIC CLIENT ID").size(9.0).strong().color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.config.microsoft_client_id).desired_width(420.0)
                        .hint_text("Application (client) ID"));
                    ui.label(RichText::new("Используется Device Code Flow. Client Secret не добавляй.")
                        .size(10.0).color(MUTED));
                });
            self.settings_save_button(ui);
        });
    }

    fn settings_storage_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Файлы и хранилище").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    let paths = [
                        ("Minecraft", self.config.game_dir.clone()),
                        ("Изолированные экземпляры", self.config.game_dir.join("instances")),
                        ("Java Runtime", config::data_dir().map(|p| p.join("runtimes")).unwrap_or_default()),
                        ("Журналы", config::data_dir().map(|p| p.join("logs")).unwrap_or_default()),
                    ];
                    for (name, path) in paths {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(name).strong().color(TEXT));
                                ui.label(RichText::new(path.display().to_string()).size(9.0).color(MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Открыть").clicked() {
                                    let _ = std::fs::create_dir_all(&path);
                                    let _ = open::that(&path);
                                }
                            });
                        });
                        ui.separator();
                    }
                    if ui.button("Открыть настройки Minty").clicked() {
                        if let Ok(path) = config::config_file() { let _ = open::that(path); }
                    }
                });
        });
    }

    fn settings_security_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Безопасность").size(21.0).strong().color(TEXT));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.label(RichText::new("Контроль загрузок").strong().color(self.accent_color()));
                    ui.label(RichText::new("Minty проверяет хеши там, где их публикует источник, и отклоняет файлы при несовпадении.")
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("Проверка модов").strong().color(self.accent_color()));
                    ui.label(RichText::new("Сканер читает метаданные JAR, ищет повреждённые архивы, повторные ID и часть конфликтов загрузчиков. Это не антивирус.")
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("Windows Sandbox / AppContainer").strong().color(RED));
                    ui.label(RichText::new("Ограниченный токен для Java пока не реализован. Minecraft и сторонние моды работают с правами текущего пользователя. Не запускай непроверенные моды.")
                        .size(10.0).color(MUTED));
                    ui.separator();
                    ui.label(RichText::new("Microsoft refresh token хранится в системном хранилище учётных данных, а не в settings.json.")
                        .size(10.0).color(MUTED));
                });
        });
    }

    fn settings_updates_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Обновления").size(21.0).strong().color(TEXT));
            ui.label(RichText::new(format!("Установлена версия {}", env!("CARGO_PKG_VERSION"))).size(11.0).color(MUTED));
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    if let Some(info) = &self.update_info {
                        ui.label(RichText::new(format!("Доступна версия {}", info.version)).strong().color(self.accent_color()));
                    }
                    if let Some(note) = &self.update_notice {
                        ui.label(RichText::new(note).size(10.0).color(MUTED));
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!self.update_checking, egui::Button::new("Проверить сейчас")).clicked() { self.check_updates(); }
                        if ui.add_enabled(self.update_info.is_some() && !self.update_downloading, egui::Button::new("Скачать обновление")).clicked() { self.start_update(); }
                        if ui.button("Релизы ↗").clicked() { let _ = open::that("https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases"); }
                    });
                    ui.label(RichText::new("Репозиторий приватный: автоматическая загрузка релиза требует доступа или отдельного публичного канала обновлений.")
                        .size(10.0).color(MUTED));
                });
        });
    }

    fn splash_screen(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(24));
        let message = match self.boot_stage {
            BootStage::CheckingUpdates => "Проверка обновлений лаунчера...",
            BootStage::SearchingJava => "Поиск установленной Java...",
            BootStage::Loading => "Загрузка лаунчера...",
            BootStage::Ready => "Готово",
        };
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            let height = ui.available_height();
            ui.vertical_centered(|ui| {
                ui.add_space(height * 0.20);
                ui.add(egui::Image::new(&self.logo_texture).fit_to_exact_size(egui::vec2(112.0,112.0)));
                ui.add_space(18.0);
                ui.label(RichText::new("MINTY LAUNCHER").size(24.0).strong().color(TEXT));
                ui.add_space(8.0);
                ui.label(RichText::new(message).size(13.0).color(MUTED));
                ui.add_space(22.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0,38.0), egui::Sense::hover());
                let centre = rect.center();
                ui.painter().circle_stroke(centre, 15.0, Stroke::new(2.0, PANEL_LIGHT));
                let angle = self.boot_stage_started.elapsed().as_secs_f32() * 4.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                ui.painter().line_segment(
                    [centre + direction * 9.0, centre + direction * 17.0],
                    Stroke::new(3.0, self.accent_color()),
                );
                ui.add_space(12.0);
                ui.label(RichText::new(format!("Версия {}", env!("CARGO_PKG_VERSION"))).size(9.0).color(Color32::from_rgb(108,128,125)));
            });
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.separator();
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("●").color(if self.error.is_some() { RED } else { MINT }).size(10.0));
                ui.label(RichText::new(&self.status).size(10.0).color(MUTED));
            });
        });
        if let Some((code, url, message)) = &self.device_code {
            ui.add_space(6.0);
            egui::Frame::none().fill(PANEL_LIGHT).rounding(Rounding::same(8.0)).inner_margin(egui::Margin::same(9.0)).show(ui, |ui| {
                ui.label(RichText::new(message).size(10.0).color(TEXT));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(code).size(19.0).strong().color(MINT));
                    if ui.small_button("Страница входа").clicked() { let _ = open::that(url); }
                    if ui.small_button("Копировать код").clicked() { ui.output_mut(|output| output.copied_text = code.clone()); }
                });
            });
        }
        if let Some(error) = &self.error {
            ui.add_space(5.0);
            let lower = error.to_ascii_lowercase();
            let (heading, hint) = if lower.contains("win32 error 5") || lower.contains("os error 5")
                || lower.contains("отказано в доступе") {
                ("Отказано в доступе", "Windows заблокировал запуск файла или запись в папку. Проверь права на java.exe и каталог игры; не размещай .minecraft в Program Files и проверь карантин антивируса.")
            } else if lower.contains("minecraft java edition") && lower.contains("лиценз") {
                ("Не найдена лицензия Minecraft Java", "Войди в Microsoft-аккаунт, на котором куплена Java Edition. Офлайн-профиль подходит только для одиночной игры.")
            } else if lower.contains("java runtime") || lower.contains("java не найдена") {
                ("Проблема с Java", "Minty не нашёл подходящую Java. Проверь интернет или укажи корректный java.exe в настройках.")
            } else if lower.contains("не удалось загрузить") || lower.contains("timed out") || lower.contains("connection") {
                ("Ошибка загрузки", "Проверь соединение, свободное место на диске и доступ к серверам Mojang/Modrinth.")
            } else if lower.contains("поврежд") || lower.contains("checksum") || lower.contains("контрольная сумма") {
                ("Файл повреждён", "Загрузка не прошла проверку целостности. Повтори загрузку; повреждённый файл не был принят.")
            } else if lower.contains("отменён") {
                ("Запуск отменён", "Подготовка игры остановлена по твоему запросу. Уже загруженные файлы останутся на диске.")
            } else {
                ("Не удалось выполнить действие", "Открой подробности ниже. Полный текст ошибки можно скопировать и отправить в поддержку.")
            };
            egui::Frame::none().fill(Color32::from_rgb(55, 35, 38))
                .rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(heading).strong().color(RED));
                    ui.label(RichText::new(hint).size(10.0).color(TEXT));
                    egui::CollapsingHeader::new("Подробности ошибки")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.label(RichText::new(error).monospace().size(9.0).color(MUTED));
                            if ui.small_button("Копировать текст ошибки").clicked() {
                                ui.output_mut(|output| output.copied_text = error.clone());
                            }
                        });
                });
        }
    }
}

impl eframe::App for MintyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_events();
        self.window_titlebar(ctx);
        if self.boot_stage != BootStage::Ready {
            self.splash_screen(ctx);
            return;
        }
        self.top_navigation(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(21.0)))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("status_footer")
                    .frame(egui::Frame::none().fill(BG))
                    .show_inside(ui, |ui| self.footer(ui));
                let transition = if self.config.enable_animations {
                    (self.page_transition_started.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0)
                } else { 1.0 };
                ui.set_opacity(transition);
                ui.add_space((1.0 - transition) * 10.0);
                match self.page {
                    Page::Home => self.home_page(ui),
                    Page::Minecraft => self.minecraft_page(ui),
                    Page::Settings => self.settings_hub(ui),
                }
                if transition < 1.0 { ctx.request_repaint_after(Duration::from_millis(16)); }
            });
        if self.auth_busy || self.launch_busy || self.install_busy || self.version_rx.is_some()
            || self.update_checking || self.update_downloading || self.mod_searching || self.mod_installing || self.benchmark_busy {
            let interval = if self.config.weak_pc_mode { 500 } else { 180 };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
        if self.restart_for_update || self.close_after_launch {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.auth_busy && self.device_code.is_none() {
            egui::Area::new(egui::Id::new("auth_busy_overlay"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 100.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new("Ожидание ответа Microsoft…").color(MINT).strong());
                });
        }
    }
}

fn scan_installed_profiles(game_dir: &std::path::Path) -> Vec<InstalledProfile> {
    let root = game_dir.join("versions");
    let entries = match std::fs::read_dir(&root) { Ok(v) => v, Err(_) => return Vec::new() };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let folder = entry.path();
        if !folder.is_dir() { continue; }
        let id = entry.file_name().to_string_lossy().into_owned();
        let json_path = folder.join(format!("{id}.json"));
        if !json_path.is_file() { continue; }
        let raw = match std::fs::read_to_string(json_path) { Ok(v) => v, Err(_) => continue };
        let metadata: serde_json::Value = match serde_json::from_str(&raw) { Ok(v) => v, Err(_) => continue };
        let game_version = metadata.get("inheritsFrom").and_then(serde_json::Value::as_str).unwrap_or(&id).to_owned();
        let base = root.join(&game_version);
        if !base.join(format!("{game_version}.json")).is_file() || !base.join(format!("{game_version}.jar")).is_file() { continue; }
        let lower = id.to_ascii_lowercase();
        let loader = if lower.contains("neoforge") { "NeoForge" }
            else if lower.contains("forge") { "Forge" }
            else if lower.contains("fabric") { "Fabric" }
            else if lower.contains("quilt") { "Quilt" }
            else { "Vanilla" };
        result.push(InstalledProfile { id, game_version, loader: loader.to_owned() });
    }
    result.sort_by(|a,b| b.id.cmp(&a.id));
    result
}

fn skin_region_rects(region:&str)->Vec<(usize,usize,usize,usize)>{
    match region{
        "Голова"=>vec![(0,0,32,16)],
        "Тело"=>vec![(16,16,24,16)],
        "Руки"=>vec![(40,16,16,16),(32,48,16,16)],
        "Ноги"=>vec![(0,16,16,16),(16,48,16,16)],
        _=>vec![(0,0,64,64)],
    }
}


fn translate_label(language: &str, key: &str) -> &'static str {
    let index = match language {
        "English" => 1,
        "Deutsch" => 2,
        "Français" => 3,
        "Español" => 4,
        "Polski" => 5,
        "Українська" => 6,
        "Türkçe" => 7,
        "Português" => 8,
        "日本語" => 9,
        _ => 0,
    };
    let values: [&'static str; 10] = match key {
        "home" => ["Главная", "Home", "Startseite", "Accueil", "Inicio", "Główna", "Головна", "Ana sayfa", "Início", "ホーム"],
        "minecraft" => ["Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft"],
        "settings" => ["Настройки", "Settings", "Einstellungen", "Paramètres", "Ajustes", "Ustawienia", "Налаштування", "Ayarlar", "Configurações", "設定"],
        "microsoft" => ["Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft", "Microsoft"],
        "offline" => ["Офлайн", "Offline", "Offline", "Hors ligne", "Sin conexión", "Offline", "Çevrimdışı", "Çevrimdışı", "Offline", "オフライン"],
        "builds" => ["Сборки", "Profiles", "Profile", "Profils", "Perfiles", "Profile", "Profiller", "Perfis", "Perfis", "プロファイル"],
        "versions" => ["Версии", "Versions", "Versionen", "Versions", "Versiones", "Wersje", "Sürümler", "Versões", "Versões", "バージョン"],
        "profile" => ["Профиль", "Profile", "Profil", "Profil", "Perfil", "Profil", "Profil", "Perfil", "Perfil", "プロフィール"],
        "mods" => ["Моды", "Mods", "Mods", "Mods", "Mods", "Mody", "Modlar", "Mods", "Mods", "MOD"],
        "bedrock" => ["Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock", "Bedrock"],
        "general" => ["Основные", "General", "Allgemein", "Général", "General", "Ogólne", "Genel", "Geral", "Geral", "一般"],
        "game" => ["Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft", "Minecraft"],
        "java" => ["Java", "Java", "Java", "Java", "Java", "Java", "Java", "Java", "Java", "Java"],
        "performance" => ["Производительность", "Performance", "Leistung", "Performances", "Rendimiento", "Wydajność", "Performans", "Desempenho", "Desempenho", "パフォーマンス"],
        "appearance" => ["Оформление", "Appearance", "Darstellung", "Apparence", "Apariencia", "Wygląd", "Görünüm", "Aparência", "Aparência", "外観"],
        "account" => ["Аккаунт", "Account", "Konto", "Compte", "Cuenta", "Konto", "Hesap", "Conta", "Conta", "アカウント"],
        "storage" => ["Хранилище", "Storage", "Speicher", "Stockage", "Almacenamiento", "Pamięć", "Depolama", "Armazenamento", "Armazenamento", "ストレージ"],
        "security" => ["Безопасность", "Security", "Sicherheit", "Sécurité", "Seguridad", "Bezpieczeństwo", "Güvenlik", "Segurança", "Segurança", "セキュリティ"],
        "updates" => ["Обновления", "Updates", "Updates", "Mises à jour", "Actualizaciones", "Aktualizacje", "Güncellemeler", "Atualizações", "Atualizações", "更新"],
        "journal" => ["Журнал", "Logs", "Protokoll", "Journal", "Registro", "Dziennik", "Günlük", "Registro", "Registro", "ログ"],
        _ => ["", "", "", "", "", "", "", "", "", ""],
    };
    values[index]
}

fn configure_style(ctx: &egui::Context, accent: Color32) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(10, 14, 19);
    style.visuals.faint_bg_color = PANEL;
    style.visuals.selection.bg_fill = accent;
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(
        accent.r().saturating_add(12).min(255),
        accent.g().saturating_add(12).min(255),
        accent.b().saturating_add(12).min(255),
    );
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.bg_fill = accent;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.window_rounding = Rounding::same(16.0);
    style.visuals.menu_rounding = Rounding::same(10.0);
    style.visuals.override_text_color = Some(TEXT);
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(13.0, 9.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(12.5));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(22.0));
    ctx.set_style(style);
}

fn field_label(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).size(10.0).strong().color(MUTED));
    ui.add_space(5.0);
}

fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{skin_region_rects, valid_username};

    #[test]
    fn validates_offline_usernames() {
        assert!(valid_username("Steve_123"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("two words"));
        assert!(!valid_username("Иван"));
    }

    #[test]
    fn skin_region_shortcuts_cover_uv_atlas_parts(){
        assert_eq!(skin_region_rects("Голова"),vec![(0,0,32,16)]);
        assert_eq!(skin_region_rects("Руки").len(),2);
        assert_eq!(skin_region_rects("Вся текстура"),vec![(0,0,64,64)]);
    }
}

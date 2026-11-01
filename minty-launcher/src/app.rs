use crate::{
    auth::{self, Account, AuthEvent},
    config::{self, AppConfig},
    minecraft::{self, LaunchEvent, VersionInfo},
    skin,
    mods::{self, ModProject},
    bedrock,
    system_info::{self, HardwareInfo},
    updater::{self, UpdateInfo},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Rounding, Stroke};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread,
    time::Duration,
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
    General,
    Journal,
}

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
            mod_status: "Ищи моды через Modrinth или импортируй собственные файлы.".to_owned(),
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
            skin_status: "Импортируй PNG-скин или начни рисовать на пустом холсте".to_owned(),
            ctx: cc.egui_ctx.clone(),
        };
        app.load_versions();
        app.refresh_account();
        app.check_updates();
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

    fn poll_background_tasks(&mut self) {
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

    fn search_mod_store(&mut self) {
        if self.mod_searching || self.mod_installing { return; }
        let query = self.mod_search.trim().to_owned();
        let game_version = self.config.selected_version.clone();
        let loader = self.config.selected_loader.clone();
        let (tx, rx) = unbounded();
        self.mod_search_rx = Some(rx);
        self.mod_searching = true;
        self.mod_status = "Ищу совместимые моды в каталоге Modrinth…".to_owned();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            tx.send(mods::search(&query, &game_version, &loader).map_err(|error| format!("{error:#}"))).ok();
            ctx.request_repaint();
        });
    }

    fn install_store_mod(&mut self, slug: String) {
        if self.mod_installing { return; }
        if self.config.selected_loader == "Vanilla" {
            self.mod_status = "Для модов сначала выбери Fabric, Forge, NeoForge или Quilt.".to_owned();
            return;
        }
        let game_version = self.config.selected_version.clone();
        let loader = self.config.selected_loader.clone();
        let mods_dir = self.config.game_dir.join("mods");
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
                    let mods_dir = self.config.game_dir.join("mods");
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

    fn open_java_mods_folder(&self) {
        let path = self.config.game_dir.join("mods");
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
            self.page = Page::Settings;
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

    fn accent_color(&self) -> Color32 {
        Color32::from_rgb(
            self.config.accent_color[0],
            self.config.accent_color[1],
            self.config.accent_color[2],
        )
    }

    fn avatar_widget(&self, ui: &mut egui::Ui, size: f32) {
        if let Some(texture) = &self.avatar_texture {
            ui.add(egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(size, size))
                .texture_options(egui::TextureOptions::NEAREST));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), size * 0.48, self.accent_color());
            let name = self.account.as_ref().map(|account| account.name.as_str())
                .unwrap_or(self.config.offline_username.as_str());
            let initial = name.chars().next().unwrap_or('M').to_uppercase().to_string();
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial,
                FontId::proportional(size * 0.42), Color32::from_rgb(16, 30, 27));
        }
    }

    fn top_navigation(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("minty_top_navigation")
            .exact_height(68.0)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(22.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                    ui.painter().rect_filled(logo, Rounding::same(11.0), self.accent_color());
                    ui.painter().text(logo.center(), egui::Align2::CENTER_CENTER, "M",
                        FontId::proportional(24.0), Color32::from_rgb(15, 31, 25));
                    ui.add_space(7.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("MINTY").size(18.0).strong().color(TEXT));
                        ui.label(RichText::new("JAVA  ·  BEDROCK").size(8.0).strong().color(MUTED));
                    });
                    ui.add_space(26.0);
                    self.top_tab(ui, "Главная", Page::Home);
                    self.top_tab(ui, "Minecraft", Page::Minecraft);
                    self.top_tab(ui, "Настройки", Page::Settings);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.launch_busy {
                            let cancel = egui::Button::new(RichText::new("Отмена запуска Minecraft").strong().color(Color32::WHITE))
                                .fill(RED).rounding(Rounding::same(9.0));
                            if ui.add(cancel).clicked() { self.cancel_launch(); }
                            ui.add_space(10.0);
                        }
                        ui.vertical(|ui| {
                            let name = self.account.as_ref().map(|account| account.name.as_str())
                                .unwrap_or(self.config.offline_username.as_str());
                            ui.label(RichText::new(name).strong().color(TEXT).size(11.0));
                            ui.label(RichText::new(if self.account.is_some() { "Microsoft" } else { "Офлайн" })
                                .size(9.0).color(MUTED));
                        });
                        ui.add_space(8.0);
                        self.avatar_widget(ui, 36.0);
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
        if response.clicked() { self.page = page; }
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

    fn minecraft_world_background(&self, ui: &mut egui::Ui) -> egui::Rect {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 250.0), egui::Sense::hover());
        let painter = ui.painter();
        let sky = Color32::from_rgb(73, 119, 143);
        painter.rect_filled(rect, Rounding::same(20.0), sky);

        // Pixel-art clouds and a warm square sun.
        let sun = egui::Rect::from_min_size(rect.left_top() + egui::vec2(rect.width() * 0.76, 35.0),
            egui::vec2(42.0, 42.0));
        painter.rect_filled(sun, Rounding::same(0.0), Color32::from_rgb(246, 208, 132));
        let cloud = Color32::from_rgb(198, 215, 211);
        for (x, y, w) in [(0.55, 0.17, 72.0), (0.60, 0.13, 58.0), (0.84, 0.30, 68.0)] {
            let base = rect.left_top() + egui::vec2(rect.width() * x, rect.height() * y);
            painter.rect_filled(egui::Rect::from_min_size(base, egui::vec2(w, 13.0)), Rounding::same(0.0), cloud);
            painter.rect_filled(egui::Rect::from_min_size(base + egui::vec2(15.0, -10.0), egui::vec2(w * 0.58, 11.0)), Rounding::same(0.0), cloud);
        }

        let mountain_a = vec![
            rect.left_bottom() + egui::vec2(0.0, -58.0),
            rect.left_top() + egui::vec2(rect.width() * 0.19, 76.0),
            rect.left_top() + egui::vec2(rect.width() * 0.34, 112.0),
            rect.left_top() + egui::vec2(rect.width() * 0.47, 151.0),
            rect.right_bottom() + egui::vec2(0.0, -42.0),
        ];
        painter.add(egui::Shape::convex_polygon(mountain_a, Color32::from_rgb(84, 112, 111), Stroke::NONE));
        let mountain_b = vec![
            rect.left_top() + egui::vec2(rect.width() * 0.45, 155.0),
            rect.left_top() + egui::vec2(rect.width() * 0.62, 89.0),
            rect.left_top() + egui::vec2(rect.width() * 0.76, 132.0),
            rect.right_top() + egui::vec2(0.0, 150.0),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        painter.add(egui::Shape::convex_polygon(mountain_b, Color32::from_rgb(64, 98, 91), Stroke::NONE));

        // Foreground terrain built from deliberately blocky layers.
        let horizon = rect.top() + rect.height() * 0.72;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), horizon), rect.right_bottom()),
            Rounding::same(0.0), Color32::from_rgb(73, 125, 78));
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), horizon + 13.0), rect.right_bottom()),
            Rounding::same(0.0), Color32::from_rgb(98, 73, 54));
        let block = Color32::from_rgb(90, 142, 82);
        for index in 0..13 {
            let x = rect.left() + index as f32 * (rect.width() / 12.0);
            let y = horizon + if index % 3 == 0 { -8.0 } else { 0.0 };
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(19.0, 13.0)), Rounding::same(0.0), block);
        }

        // Low-poly block trees at both sides, leaving the centre readable for the greeting.
        for (tx, ty, scale) in [(0.86, 0.72, 1.0), (0.93, 0.76, 0.78), (0.08, 0.72, 0.72)] {
            let x = rect.left() + rect.width() * tx;
            let y = rect.top() + rect.height() * ty;
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y - 14.0 * scale), egui::vec2(9.0 * scale, 38.0 * scale)),
                Rounding::same(0.0), Color32::from_rgb(102, 77, 54));
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y - 43.0 * scale), egui::vec2(42.0 * scale, 34.0 * scale)),
                Rounding::same(0.0), Color32::from_rgb(58, 116, 73));
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x - 10.0 * scale, y - 56.0 * scale), egui::vec2(22.0 * scale, 18.0 * scale)),
                Rounding::same(0.0), Color32::from_rgb(77, 135, 83));
        }

        let account = self.account.as_ref().map(|value| value.name.as_str())
            .unwrap_or(self.config.offline_username.as_str());
        let greeting = format!("Привет, {account}!");
        painter.text(
            rect.left_top() + egui::vec2(28.0, 28.0),
            egui::Align2::LEFT_TOP,
            "MINTY  /  MINECRAFT",
            FontId::proportional(10.0),
            Color32::from_rgb(225, 241, 233),
        );
        painter.text(
            rect.left_top() + egui::vec2(28.0, 63.0),
            egui::Align2::LEFT_TOP,
            greeting,
            FontId::proportional(31.0),
            Color32::WHITE,
        );
        painter.text(
            rect.left_top() + egui::vec2(29.0, 111.0),
            egui::Align2::LEFT_TOP,
            "Твой мир начинается здесь.",
            FontId::proportional(14.0),
            Color32::from_rgb(239, 245, 237),
        );
        rect
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            self.minecraft_world_background(ui);
            ui.add_space(15.0);

            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(15.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ВЫБЕРИ СВОЮ ИГРУ").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("home_version_picker")
                                .selected_text(if self.config.selected_version.is_empty() { "Загрузка…" } else { &self.config.selected_version })
                                .width(180.0).show_ui(ui, |ui| {
                                    if !self.latest_release.is_empty() {
                                        ui.selectable_value(&mut self.config.selected_version, self.latest_release.clone(),
                                            format!("{} · последняя", self.latest_release));
                                    }
                                    for version in self.versions.iter().take(180) {
                                        ui.selectable_value(&mut self.config.selected_version, version.id.clone(), version.id.clone());
                                    }
                                });
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ЗАГРУЗЧИК").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("home_loader_picker")
                                .selected_text(&self.config.selected_loader).width(145.0).show_ui(ui, |ui| {
                                    for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                        if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                            self.config.selected_profile_id.clear();
                                            self.config.selected_loader_version.clear();
                                        }
                                    }
                                });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Открыть папку игры").clicked() {
                                let _ = std::fs::create_dir_all(&self.config.game_dir);
                                let _ = open::that(&self.config.game_dir);
                            }
                            if ui.button("Сохранить выбор").clicked() {
                                self.config.selected_profile_id.clear();
                                let _ = config::save(&self.config);
                            }
                        });
                    });
                });

            ui.add_space(13.0);
            ui.columns(2, |columns| {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(15.0)).show(&mut columns[0], |ui| {
                        ui.horizontal(|ui| {
                            self.avatar_widget(ui, 48.0);
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("ПРОФИЛЬ").size(9.0).strong().color(MUTED));
                                let name = self.account.as_ref().map(|account| account.name.as_str())
                                    .unwrap_or(self.config.offline_username.as_str());
                                ui.label(RichText::new(name).size(17.0).strong().color(TEXT));
                                ui.label(RichText::new(if self.account.is_some() { "Microsoft · лицензия подтверждена" } else { "Офлайн-профиль" })
                                    .size(10.0).color(MUTED));
                            });
                        });
                        ui.add_space(8.0);
                        if ui.button("Открыть профиль").clicked() {
                            self.page = Page::Minecraft;
                            self.minecraft_tab = MinecraftTab::Profile;
                        }
                    });
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(15.0)).show(&mut columns[1], |ui| {
                        ui.label(RichText::new("ТВОЯ СИСТЕМА").size(9.0).strong().color(MUTED));
                        ui.add_space(4.0);
                        ui.label(RichText::new(&self.hardware.cpu_name).size(11.0).strong().color(TEXT));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} GB ОЗУ", self.hardware.total_memory_gb)).strong().color(TEXT));
                            ui.label(RichText::new(format!("{} потоков CPU", self.hardware.logical_cores)).color(MUTED));
                        });
                        ui.label(RichText::new(format!("Автонастройка Java: {} GB", self.hardware.recommended_memory_gb))
                            .size(10.0).color(self.accent_color()));
                    });
            });

            if let Some(info) = self.update_info.clone() {
                ui.add_space(13.0);
                egui::Frame::none().fill(Color32::from_rgb(36, 54, 49))
                    .rounding(Rounding::same(15.0)).inner_margin(egui::Margin::same(14.0))
                    .stroke(Stroke::new(1.0, self.accent_color())).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new("ДОСТУПНО ОБНОВЛЕНИЕ").size(9.0).strong().color(self.accent_color()));
                                ui.label(RichText::new(format!("Minty Launcher {}", info.version)).size(17.0).strong().color(TEXT));
                                ui.label(RichText::new("Автоматическая установка, ручная загрузка или пропуск версии.")
                                    .size(10.0).color(MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Пропустить").clicked() { self.skip_update(); }
                                if ui.small_button("Скачать вручную").clicked() { self.open_manual_update(); }
                                if ui.add_enabled(!self.update_downloading,
                                    egui::Button::new(if self.update_downloading { "Загрузка…" } else { "Обновить автоматически" })
                                        .fill(self.accent_color()).rounding(Rounding::same(9.0))).clicked() {
                                    self.start_update();
                                }
                            });
                        });
                    });
            } else if let Some(notice) = &self.update_notice {
                if notice.contains("приватный") || notice.contains("HTTP 404") {
                    ui.add_space(10.0);
                    ui.label(RichText::new(notice).size(10.0).color(MUTED));
                }
            }

            ui.add_space(15.0);
            ui.horizontal(|ui| {
                let play = egui::Button::new(
                    RichText::new(if self.launch_busy { "ПОДГОТОВКА MINECRAFT…" } else { "▶  ИГРАТЬ" })
                        .size(14.0).strong().color(Color32::from_rgb(15, 35, 28)))
                    .fill(self.accent_color())
                    .min_size(egui::vec2(230.0, 50.0))
                    .rounding(Rounding::same(12.0));
                if ui.add_enabled(!self.launch_busy && !self.install_busy, play).clicked() {
                    self.start_game();
                }
                if self.launch_busy {
                    if ui.add_sized([230.0, 50.0],
                        egui::Button::new(RichText::new("Отмена запуска Minecraft").strong().color(Color32::WHITE))
                            .fill(RED).rounding(Rounding::same(12.0))).clicked() {
                        self.cancel_launch();
                    }
                } else if ui.button("Открыть Minecraft").clicked() {
                    self.page = Page::Minecraft;
                }
            });
            ui.add_space(7.0);
            ui.label(RichText::new("Java Edition · Vanilla, Forge, NeoForge, Fabric и Quilt")
                .size(10.0).color(MUTED));
        });
    }

    fn minecraft_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("MINECRAFT").size(9.0).strong().color(self.accent_color()));
                ui.label(RichText::new("Игровая библиотека").size(27.0).strong().color(TEXT));
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
            self.minecraft_tab_button(ui, "Сборки", MinecraftTab::Builds);
            self.minecraft_tab_button(ui, "Версии", MinecraftTab::Versions);
            self.minecraft_tab_button(ui, "Профиль", MinecraftTab::Profile);
            self.minecraft_tab_button(ui, "Моды", MinecraftTab::Mods);
            self.minecraft_tab_button(ui, "Bedrock", MinecraftTab::Bedrock);
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
        if response.clicked() { self.minecraft_tab = tab; }
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
        ui.label(RichText::new("Профиль Minecraft").size(23.0).strong().color(TEXT));
        ui.label(RichText::new("Аккаунт, аватар и внешний вид персонажа.").size(12.0).color(MUTED));
        ui.add_space(12.0);
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(15.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.avatar_widget(ui, 58.0);
                    ui.add_space(8.0);
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
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.mod_search)
                            .desired_width(350.0).hint_text("Найти мод: Sodium, JEI, Xaero's…"));
                        if ui.add_enabled(!self.mod_searching && !self.mod_installing,
                            egui::Button::new(if self.mod_searching { "Ищу…" } else { "Найти на Modrinth" })
                                .fill(self.accent_color()).rounding(Rounding::same(8.0))).clicked() {
                            self.search_mod_store();
                        }
                    });
                    ui.label(RichText::new("Для установки модов нужен Fabric, Forge, NeoForge или Quilt. Modrinth покажет совместимые версии.")
                        .size(10.0).color(MUTED));
                    ui.label(RichText::new(&self.mod_status).size(10.0).color(self.accent_color()));
                });
            ui.add_space(12.0);
            let projects = self.mods.clone();
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                .inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(390.0).show(ui, |ui| {
                        if projects.is_empty() {
                            ui.label(RichText::new("Каталог пока пуст. Введи название и нажми «Найти на Modrinth».")
                                .color(MUTED));
                        }
                        let mut requested = None;
                        for project in projects {
                            ui.horizontal(|ui| {
                                if let Some(icon_url) = &project.icon_url {
                                    let response = ui.add(egui::Image::new(egui::ImageSource::Uri(icon_url.clone().into()))
                                        .fit_to_exact_size(egui::vec2(42.0, 42.0)));
                                    if !response.clicked() { /* icon is intentionally non-interactive */ }
                                } else {
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, Rounding::same(9.0), PANEL_LIGHT);
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "M",
                                        FontId::proportional(22.0), self.accent_color());
                                }
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&project.title).strong().color(TEXT).size(13.0));
                                    let description: String = project.description.chars().take(115).collect();
                                    ui.label(RichText::new(description).size(10.0).color(MUTED));
                                    ui.label(RichText::new(format!("{} загрузок", project.downloads)).size(9.0).color(self.accent_color()));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_enabled(!self.mod_installing,
                                        egui::Button::new(if self.mod_installing { "Установка…" } else { "Установить" })
                                            .rounding(Rounding::same(8.0))).clicked() {
                                        requested = Some(project.slug.clone());
                                    }
                                    if ui.small_button("Страница ↗").clicked() {
                                        let _ = open::that(format!("https://modrinth.com/mod/{}", project.slug));
                                    }
                                });
                            });
                            ui.add_space(7.0);
                            ui.separator();
                        }
                        if let Some(slug) = requested { self.install_store_mod(slug); }
                    });
                });
            ui.add_space(8.0);
            ui.label(RichText::new("Каталог использует публичный API Modrinth. Проверяй лицензию, описание и совместимость модов до установки.")
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
                        if ui.add_sized([220.0, 43.0],
                            egui::Button::new(RichText::new("▶  Запустить Bedrock").strong().color(Color32::from_rgb(15, 35, 28)))
                                .fill(self.accent_color()).rounding(Rounding::same(10.0))).clicked() {
                            match bedrock::launch() {
                                Ok(()) => self.log("Передана команда запуска Minecraft Bedrock."),
                                Err(error) => self.error = Some(format!("{error:#}")),
                            }
                        }
                        if ui.button("Открыть Microsoft Store").clicked() {
                            if let Err(error) = bedrock::open_store_search() { self.error = Some(format!("{error:#}")); }
                        }
                        if ui.button("Импортировать дополнение").clicked() { self.import_game_files(); }
                    });
                });
            ui.add_space(12.0);
            ui.label(RichText::new("НАЙДЕННЫЕ ДАННЫЕ BEDROCK").size(9.0).strong().color(MUTED));
            let installations = bedrock::installations();
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
            ui.label(RichText::new("Bedrock Stable и Preview обновляются и выбираются через Microsoft Store. Minty не подменяет версии Store и не скачивает неподдерживаемые Appx-пакеты.")
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
        ui.add_space(7.0);
        ui.label(RichText::new("Skin Studio").size(27.0).strong().color(TEXT));
        ui.label(RichText::new("Редактируй текстуру своего персонажа даже без входа в Microsoft.").size(13.0).color(MUTED));
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            if ui.button("Импорт PNG").clicked() { self.import_skin(); }
            if ui.button("Экспорт PNG").clicked() { self.export_skin(); }
            if ui.button("Очистить холст").clicked() {
                self.skin_pixels = skin::blank();
                self.skin_texture = None;
                self.skin_status = "Создан пустой холст 64×64".to_owned();
            }
        });
        ui.add_space(10.0);
        ui.columns(2, |columns| {
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0)).inner_margin(egui::Margin::same(14.0)).show(&mut columns[0], |ui| {
                ui.label(RichText::new("ТЕКСТУРА 64 × 64").size(10.0).strong().color(MUTED));
                ui.add_space(8.0);
                self.skin_canvas(ui);
            });
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0)).inner_margin(egui::Margin::same(16.0)).show(&mut columns[1], |ui| {
                ui.label(RichText::new("ИНСТРУМЕНТЫ").size(10.0).strong().color(MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new("Цвет кисти").color(TEXT));
                ui.color_edit_button_srgba(&mut self.skin_color);
                ui.add_space(8.0);
                ui.label(RichText::new("Быстрая палитра").size(11.0).color(MUTED));
                ui.horizontal(|ui| {
                    for color in [
                        Color32::from_rgb(35, 35, 42),
                        Color32::from_rgb(235, 190, 145),
                        Color32::from_rgb(75, 125, 200),
                        Color32::from_rgb(180, 65, 75),
                        Color32::from_rgb(85, 165, 105),
                        Color32::from_rgb(240, 210, 90),
                    ] {
                        if ui.add_sized([25.0, 25.0], egui::Button::new(" ").fill(color)).clicked() {
                            self.skin_color = color;
                        }
                    }
                });
                if ui.small_button("Ластик — прозрачный пиксель").clicked() {
                    self.skin_color = Color32::TRANSPARENT;
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Как рисовать").strong().color(TEXT));
                ui.label(RichText::new("Зажми левую кнопку мыши на текстуре. Каждый пиксель — отдельная точка скина.").size(11.0).color(MUTED));
                ui.add_space(12.0);
                ui.label(RichText::new(&self.skin_status).size(10.0).color(MINT));
                ui.add_space(14.0);
                ui.label(RichText::new("Офлайн-совместимость").strong().color(TEXT));
                ui.label(RichText::new("Редактор работает локально и не требует аккаунта. PNG можно сохранить и использовать в моде загрузки скинов. Чистый Vanilla Minecraft не применяет локальные PNG-скины к офлайн-профилю автоматически.").size(10.0).color(MUTED));
            });
        });
    }

    fn skin_canvas(&mut self, ui: &mut egui::Ui) {
        let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &self.skin_pixels);
        if let Some(texture) = self.skin_texture.as_mut() {
            texture.set(image, egui::TextureOptions::NEAREST);
        } else {
            self.skin_texture = Some(ui.ctx().load_texture("skin-editor", image, egui::TextureOptions::NEAREST));
        }
        let size = egui::vec2(384.0, 384.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, Rounding::same(3.0), PANEL_LIGHT);
        if let Some(texture) = self.skin_texture.as_ref() {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        for index in 0..=64 {
            let x = rect.left() + rect.width() * index as f32 / 64.0;
            let y = rect.top() + rect.height() * index as f32 / 64.0;
            let color = if index % 8 == 0 { Color32::from_white_alpha(45) } else { Color32::from_white_alpha(14) };
            ui.painter().line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(0.5, color));
            ui.painter().line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], Stroke::new(0.5, color));
        }
        if (response.clicked() || response.dragged()) && response.hovered() {
            if let Some(position) = response.interact_pointer_pos() {
                let x = (((position.x - rect.left()) / rect.width()) * 64.0).floor() as i32;
                let y = (((position.y - rect.top()) / rect.height()) * 64.0).floor() as i32;
                if (0..64).contains(&x) && (0..64).contains(&y) {
                    let offset = ((y as usize * 64 + x as usize) * 4) as usize;
                    self.skin_pixels[offset..offset + 4].copy_from_slice(&self.skin_color.to_array());
                    self.skin_status = format!("Изменён пиксель: {x}, {y}");
                    ui.ctx().request_repaint();
                }
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
        ui.add_space(15.0);
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
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("СИСТЕМА").size(9.0).strong().color(self.accent_color()));
                ui.label(RichText::new("Настройки лаунчера").size(25.0).strong().color(TEXT));
            });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            self.settings_tab_button(ui, "Настройки", SettingsTab::General);
            self.settings_tab_button(ui, "Журнал", SettingsTab::Journal);
        });
        ui.add_space(8.0);
        match self.settings_tab {
            SettingsTab::General => self.settings_page(ui),
            SettingsTab::Journal => self.journal_page(ui),
        }
    }

    fn settings_tab_button(&mut self, ui: &mut egui::Ui, label: &str, tab: SettingsTab) {
        let selected = self.settings_tab == tab;
        let response = ui.add_sized([116.0, 36.0],
            egui::Button::new(RichText::new(label).strong().color(if selected { TEXT } else { MUTED }))
                .fill(if selected { PANEL_LIGHT } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                .rounding(Rounding::same(9.0)));
        if response.clicked() { self.settings_tab = tab; }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(response.rect.left_bottom() - egui::vec2(0.0, 2.0),
                    egui::vec2(response.rect.width(), 2.0)),
                Rounding::same(1.0), self.accent_color());
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(7.0);
            ui.label(RichText::new("Настройки").size(28.0).strong().color(TEXT));
            ui.label(RichText::new("Настрой папки, ресурсы компьютера, профиль и обновления.")
                .size(13.0).color(MUTED));
            ui.add_space(15.0);

            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(17.0)).show(ui, |ui| {
                    ui.label(RichText::new("АКЦЕНТНЫЙ ЦВЕТ").size(9.0).strong().color(MUTED));
                    ui.horizontal(|ui| {
                        let mut accent = self.accent_color();
                        if ui.color_edit_button_srgba(&mut accent).changed() {
                            self.config.accent_color = [accent.r(), accent.g(), accent.b()];
                            configure_style(ui.ctx(), accent);
                        }
                        ui.label(RichText::new(format!("#{:02X}{:02X}{:02X}", self.config.accent_color[0],
                            self.config.accent_color[1], self.config.accent_color[2])).monospace().color(TEXT));
                        if ui.small_button("Minty по умолчанию").clicked() {
                            self.config.accent_color = [145, 226, 183];
                            configure_style(ui.ctx(), self.accent_color());
                        }
                    });
                    ui.add_space(13.0);
                    ui.label(RichText::new("ПАПКА MINECRAFT").size(9.0).strong().color(MUTED));
                    ui.add_space(7.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.directory_input).desired_width(460.0));
                        if ui.button("Обзор…").clicked() { self.choose_game_directory(); }
                        if ui.button("Открыть").clicked() {
                            let _ = std::fs::create_dir_all(PathBuf::from(self.directory_input.trim()));
                            let _ = open::that(self.directory_input.trim());
                        }
                    });
                    ui.label(RichText::new("Папка должна быть доступна для записи. Для надёжности не используй Program Files.")
                        .size(10.0).color(MUTED));
                    ui.add_space(15.0);
                    ui.label(RichText::new("АККАУНТ MICROSOFT").size(9.0).strong().color(MUTED));
                    ui.add_space(6.0);
                    ui.add(egui::TextEdit::singleline(&mut self.config.microsoft_client_id)
                        .desired_width(570.0).hint_text("Application (client) ID"));
                    ui.label(RichText::new("Нужен для официального входа. Используй Public client и Device Code Flow; Client Secret не добавляй.")
                        .size(10.0).color(MUTED));
                    ui.add_space(15.0);
                    ui.label(RichText::new("JAVA").size(9.0).strong().color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.config.java_path)
                        .desired_width(570.0).hint_text("Пусто — Minty установит совместимый Temurin JRE самостоятельно"));
                    ui.label(RichText::new("Можно указать java.exe или javaw.exe. Если выбран javaw.exe, лаунчер автоматически попробует парный java.exe.")
                        .size(10.0).color(MUTED));
                });

            ui.add_space(13.0);
            ui.columns(2, |columns| {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(16.0)).show(&mut columns[0], |ui| {
                        ui.label(RichText::new("ПРОИЗВОДИТЕЛЬНОСТЬ").size(9.0).strong().color(MUTED));
                        ui.add_space(8.0);
                        ui.label(RichText::new(&self.hardware.cpu_name).size(11.0).strong().color(TEXT));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{} GB", self.hardware.total_memory_gb)).size(22.0).strong().color(TEXT));
                                ui.label(RichText::new("ОЗУ установлено").size(10.0).color(MUTED));
                            });
                            ui.add_space(16.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{}", self.hardware.logical_cores)).size(22.0).strong().color(TEXT));
                                ui.label(RichText::new("Потоков CPU").size(10.0).color(MUTED));
                            });
                        });
                        ui.add_space(11.0);
                        if ui.checkbox(&mut self.config.auto_memory,
                            RichText::new("Подбирать память Java автоматически").strong().color(TEXT)).changed() {
                            if self.config.auto_memory {
                                self.config.max_memory_gb = self.hardware.recommended_memory_gb;
                            }
                        }
                        if self.config.auto_memory {
                            ui.label(RichText::new(format!("Minty выделит {} GB для игры.", self.hardware.recommended_memory_gb))
                                .size(10.0).color(MINT));
                        } else {
                            let manual_max = self.hardware.total_memory_gb.saturating_sub(2).min(16).max(2) as u8;
                            self.config.max_memory_gb = self.config.max_memory_gb.clamp(2, manual_max);
                            ui.add(egui::Slider::new(&mut self.config.max_memory_gb, 2..=manual_max).suffix(" GB"));
                        }
                        ui.add_space(8.0);
                        ui.checkbox(&mut self.config.auto_graphics_profile,
                            RichText::new("Автоматический профиль графики").strong().color(TEXT));
                        ui.label(RichText::new(if self.hardware.low_end {
                            "Обнаружен начальный уровень системы: будет включён экономный профиль."
                        } else {
                            "Для этой системы будет выбран сбалансированный профиль."
                        }).size(10.0).color(MUTED));
                        ui.checkbox(&mut self.config.weak_pc_mode,
                            RichText::new("Принудительно включить режим слабого ПК").color(TEXT));
                        ui.label(RichText::new("Снижает дальность прорисовки, эффекты и потребление ресурсов. Исходный options.txt резервируется.")
                            .size(10.0).color(MUTED));
                    });
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(16.0)).show(&mut columns[1], |ui| {
                        ui.label(RichText::new("ОБНОВЛЕНИЯ").size(9.0).strong().color(MUTED));
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Установлена версия {}", env!("CARGO_PKG_VERSION")))
                            .size(17.0).strong().color(TEXT));
                        if let Some(info) = &self.update_info {
                            ui.label(RichText::new(format!("Доступно: {}", info.version)).strong().color(MINT));
                        }
                        if let Some(notice) = &self.update_notice {
                            ui.label(RichText::new(notice).size(10.0).color(MUTED));
                        } else if self.update_checking {
                            ui.label(RichText::new("Проверка обновлений…").size(10.0).color(MUTED));
                        } else {
                            ui.label(RichText::new("Minty проверяет новые версии автоматически при запуске.")
                                .size(10.0).color(MUTED));
                        }
                        ui.add_space(8.0);
                        if ui.add_enabled(!self.update_checking, egui::Button::new("Проверить сейчас")).clicked() {
                            self.check_updates();
                        }
                        if self.update_info.is_some() {
                            if ui.add_enabled(!self.update_downloading,
                                egui::Button::new(if self.update_downloading { "Загрузка…" } else { "Обновить автоматически" })
                                    .fill(MINT).rounding(Rounding::same(8.0))).clicked() {
                                self.start_update();
                            }
                            if ui.button("Скачать вручную").clicked() { self.open_manual_update(); }
                            if ui.button("Пропустить эту версию").clicked() { self.skip_update(); }
                        } else if ui.button("Открыть страницу релизов").clicked() {
                            let _ = open::that("https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases");
                        }
                        ui.label(RichText::new("Автоматическая загрузка требует доступа к GitHub Release без авторизации. Сейчас репозиторий приватный; ручная загрузка открывается в браузере после входа в GitHub.")
                            .size(9.0).color(MUTED));
                    });
            });

            ui.add_space(14.0);
            if ui.add_sized([190.0, 42.0],
                egui::Button::new(RichText::new("СОХРАНИТЬ НАСТРОЙКИ").strong().color(Color32::from_rgb(15, 35, 28)))
                    .fill(MINT).rounding(Rounding::same(10.0))).clicked() {
                self.save_settings();
            }
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(if self.error.is_some() { RED } else { MINT }).size(10.0));
            ui.label(RichText::new(&self.status).size(10.0).color(MUTED));
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
        self.top_navigation(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(21.0)))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("status_footer")
                    .frame(egui::Frame::none().fill(BG))
                    .show_inside(ui, |ui| self.footer(ui));
                match self.page {
                    Page::Home => self.home_page(ui),
                    Page::Minecraft => self.minecraft_page(ui),
                    Page::Settings => self.settings_hub(ui),
                }
            });
        if self.auth_busy || self.launch_busy || self.install_busy || self.version_rx.is_some()
            || self.update_checking || self.update_downloading || self.mod_searching || self.mod_installing {
            let interval = if self.config.weak_pc_mode { 500 } else { 180 };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
        if self.restart_for_update {
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
    use super::valid_username;

    #[test]
    fn validates_offline_usernames() {
        assert!(valid_username("Steve_123"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("two words"));
        assert!(!valid_username("Иван"));
    }
}

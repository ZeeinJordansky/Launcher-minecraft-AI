use crate::{
    auth::{self, Account, AuthEvent},
    config::{self, AppConfig},
    minecraft::{self, LaunchEvent, VersionInfo},
    skin,
    system_info::{self, HardwareInfo},
    updater::{self, UpdateInfo},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Rounding, Stroke};
use std::{path::PathBuf, thread, time::Duration};

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
    Installations,
    Skin,
    Journal,
    Settings,
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
        configure_style(&cc.egui_ctx);
        let config = config::load().unwrap_or_default();
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
        thread::spawn(move || {
            let _ = minecraft::launch(config, account, tx);
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
                }
                LaunchEvent::Failed(message) => {
                    self.error = Some(message.clone());
                    self.log("Не удалось запустить Minecraft");
                    self.launch_busy = false;
                    self.install_busy = false;
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

    fn avatar_widget(&self, ui: &mut egui::Ui, size: f32) {
        if let Some(texture) = &self.avatar_texture {
            ui.add(egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(size, size))
                .texture_options(egui::TextureOptions::NEAREST));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), size * 0.48, MINT_DARK);
            let name = self.account.as_ref().map(|account| account.name.as_str())
                .unwrap_or(self.config.offline_username.as_str());
            let initial = name.chars().next().unwrap_or('M').to_uppercase().to_string();
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial,
                FontId::proportional(size * 0.42), TEXT);
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .exact_width(238.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::same(18.0)))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let (logo, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                        ui.painter().rect_filled(logo, Rounding::same(13.0), MINT);
                        ui.painter().text(logo.center(), egui::Align2::CENTER_CENTER, "M",
                            FontId::proportional(25.0), Color32::from_rgb(16, 34, 29));
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MINTY").size(21.0).strong().color(TEXT));
                            ui.label(RichText::new("MINECRAFT LAUNCHER").size(9.0).strong().color(MUTED));
                        });
                    });
                    ui.add_space(28.0);
                    ui.label(RichText::new("БИБЛИОТЕКА").size(9.0).strong().color(MUTED));
                    ui.add_space(8.0);
                    self.nav_button(ui, "⌂", "Главная", Page::Home);
                    self.nav_button(ui, "▦", "Версии и сборки", Page::Installations);
                    self.nav_button(ui, "◈", "Скин персонажа", Page::Skin);
                    ui.add_space(16.0);
                    ui.label(RichText::new("СИСТЕМА").size(9.0).strong().color(MUTED));
                    ui.add_space(8.0);
                    self.nav_button(ui, "≡", "Журнал событий", Page::Journal);
                    self.nav_button(ui, "⚙", "Настройки", Page::Settings);
                    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("v{}  ·  Windows x64", env!("CARGO_PKG_VERSION")))
                            .size(10.0).color(MUTED));
                        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
                            .inner_margin(egui::Margin::same(11.0)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    self.avatar_widget(ui, 42.0);
                                    ui.add_space(3.0);
                                    ui.vertical(|ui| {
                                        let name = self.account.as_ref().map(|account| account.name.as_str())
                                            .unwrap_or(self.config.offline_username.as_str());
                                        ui.label(RichText::new(name).strong().color(TEXT).size(12.0));
                                        ui.label(RichText::new(if self.account.is_some() { "Microsoft · online" } else { "Офлайн-профиль" })
                                            .size(9.0).color(if self.account.is_some() { MINT } else { MUTED }));
                                    });
                                });
                            });
                        ui.add_space(7.0);
                        ui.label(RichText::new("MINTY PROJECT").size(9.0).strong().color(MUTED));
                    });
                });
            });
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, icon: &str, label: &str, page: Page) {
        let selected = self.page == page;
        let response = ui.add_sized([200.0, 43.0],
            egui::Button::new(RichText::new(format!("{icon}    {label}"))
                .size(13.0).strong().color(if selected { TEXT } else { MUTED }))
                .fill(if selected { PANEL_LIGHT } else { Color32::TRANSPARENT })
                .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                .rounding(Rounding::same(11.0)));
        if response.clicked() { self.page = page; }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(response.rect.left_top(), egui::vec2(3.0, response.rect.height())),
                Rounding::same(2.0), MINT);
        }
        ui.add_space(4.0);
    }

    fn world_art(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(204.0, 150.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, Rounding::same(16.0), Color32::from_rgb(24, 50, 48));
        painter.circle_filled(rect.left_top() + egui::vec2(150.0, 34.0), 20.0, GOLD);
        painter.add(egui::Shape::convex_polygon(
            vec![
                rect.left_bottom() + egui::vec2(0.0, -15.0),
                rect.left_top() + egui::vec2(58.0, 54.0),
                rect.left_top() + egui::vec2(97.0, 83.0),
                rect.right_bottom() + egui::vec2(0.0, -5.0),
            ],
            Color32::from_rgb(50, 91, 82), Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(
            vec![
                rect.left_bottom() + egui::vec2(0.0, -4.0),
                rect.left_top() + egui::vec2(112.0, 70.0),
                rect.left_top() + egui::vec2(155.0, 98.0),
                rect.right_bottom(),
            ],
            Color32::from_rgb(35, 70, 63), Stroke::NONE));
        let tree_base = rect.left_bottom() + egui::vec2(32.0, -13.0);
        painter.rect_filled(egui::Rect::from_center_size(tree_base + egui::vec2(0.0, -15.0),
            egui::vec2(8.0, 30.0)), Rounding::same(2.0), Color32::from_rgb(104, 78, 56));
        painter.circle_filled(tree_base + egui::vec2(0.0, -35.0), 18.0, Color32::from_rgb(90, 147, 106));
        painter.circle_filled(tree_base + egui::vec2(12.0, -27.0), 11.0, Color32::from_rgb(111, 167, 119));
        painter.rect_filled(egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(0.0, -8.0),
            egui::vec2(rect.width(), 8.0)), Rounding::same(0.0), Color32::from_rgb(80, 126, 86));
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("ПРОСТО. БЫСТРО. ПО-ТВОЕМУ.").size(9.0).strong().color(MINT));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Добро пожаловать").size(28.0).strong().color(TEXT));
                    ui.label(RichText::new("Всё для твоего Minecraft — в одном месте.").size(13.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(!self.update_checking, egui::Button::new("↻  Проверить обновления")
                        .fill(PANEL).rounding(Rounding::same(10.0))).clicked() {
                        self.check_updates();
                    }
                });
            });
            ui.add_space(20.0);

            egui::Frame::none().fill(Color32::from_rgb(24, 40, 39))
                .rounding(Rounding::same(21.0)).inner_margin(egui::Margin::same(22.0))
                .stroke(Stroke::new(1.0, Color32::from_rgb(44, 69, 62))).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MINTY  /  JAVA EDITION").size(10.0).strong().color(MINT));
                            ui.add_space(8.0);
                            ui.label(RichText::new("Твой мир.\nТвои правила.").size(31.0).strong().color(TEXT));
                            ui.add_space(7.0);
                            ui.label(RichText::new("Запускай любимые версии, собирай модификации\nи возвращайся в игру без лишних шагов.")
                                .size(12.0).color(Color32::from_rgb(190, 207, 199)));
                            ui.add_space(17.0);
                            let version = if self.config.selected_version.is_empty() { "Загрузка списка…" } else { &self.config.selected_version };
                            ui.horizontal(|ui| {
                                egui::Frame::none().fill(Color32::from_rgb(36, 65, 57))
                                    .rounding(Rounding::same(8.0)).inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                    .show(ui, |ui| { ui.label(RichText::new(format!("Minecraft {version}")).strong().color(TEXT).size(11.0)); });
                                egui::Frame::none().fill(Color32::from_rgb(36, 65, 57))
                                    .rounding(Rounding::same(8.0)).inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                    .show(ui, |ui| { ui.label(RichText::new(self.config.selected_loader.to_uppercase()).strong().color(MINT).size(10.0)); });
                            });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.world_art(ui));
                    });
                });

            ui.add_space(13.0);
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ВЕРСИЯ ИГРЫ").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("home_version_picker")
                                .selected_text(if self.config.selected_version.is_empty() { "Загрузка…" } else { &self.config.selected_version })
                                .width(160.0).show_ui(ui, |ui| {
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
                                .selected_text(&self.config.selected_loader).width(150.0).show_ui(ui, |ui| {
                                    for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                        if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                            self.config.selected_profile_id.clear();
                                            self.config.selected_loader_version.clear();
                                        }
                                    }
                                });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Открыть каталог версий").clicked() { self.page = Page::Installations; }
                            if ui.button("↻").on_hover_text("Обновить список версий").clicked() { self.load_versions(); }
                        });
                    });
                    if ui.button("Сохранить выбор").clicked() {
                        self.config.selected_profile_id.clear();
                        let _ = config::save(&self.config);
                    }
                });

            ui.add_space(13.0);
            ui.columns(2, |columns| {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(15.0)).show(&mut columns[0], |ui| {
                        ui.horizontal(|ui| {
                            self.avatar_widget(ui, 48.0);
                            ui.add_space(5.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("ТВОЙ ПРОФИЛЬ").size(9.0).strong().color(MUTED));
                                let name = self.account.as_ref().map(|account| account.name.as_str())
                                    .unwrap_or(self.config.offline_username.as_str());
                                ui.label(RichText::new(name).size(17.0).strong().color(TEXT));
                                ui.label(RichText::new(if self.account.is_some() { "Microsoft · лицензия подтверждена" } else { "Локальный офлайн-профиль" })
                                    .size(10.0).color(if self.account.is_some() { MINT } else { MUTED }));
                            });
                        });
                        ui.add_space(10.0);
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
                        } else {
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.config.offline_username)
                                    .desired_width(125.0).hint_text("Ник игрока"));
                                if ui.button("Войти через Microsoft").clicked() { self.begin_login(); }
                            });
                        }
                    });
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                    .inner_margin(egui::Margin::same(15.0)).show(&mut columns[1], |ui| {
                        ui.label(RichText::new("ТВОЙ КОМПЬЮТЕР").size(9.0).strong().color(MUTED));
                        ui.add_space(7.0);
                        ui.label(RichText::new(&self.hardware.cpu_name).size(11.0).strong().color(TEXT));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{} GB", self.hardware.total_memory_gb)).size(21.0).strong().color(TEXT));
                                ui.label(RichText::new("Оперативная память").size(10.0).color(MUTED));
                            });
                            ui.add_space(20.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{}", self.hardware.logical_cores)).size(21.0).strong().color(TEXT));
                                ui.label(RichText::new("Логических потоков CPU").size(10.0).color(MUTED));
                            });
                        });
                        ui.add_space(9.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Рекомендовано Java").size(11.0).color(MUTED));
                            ui.label(RichText::new(format!("{} GB", self.hardware.recommended_memory_gb)).strong().color(MINT));
                            ui.label(RichText::new(if self.hardware.low_end { "· экономный профиль" } else { "· сбалансированный профиль" }).size(9.0).color(MUTED));
                        });
                    });
            });

            if let Some(info) = self.update_info.clone() {
                ui.add_space(13.0);
                egui::Frame::none().fill(Color32::from_rgb(36, 54, 49))
                    .rounding(Rounding::same(16.0)).inner_margin(egui::Margin::same(15.0))
                    .stroke(Stroke::new(1.0, MINT_DARK)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new("ДОСТУПНО ОБНОВЛЕНИЕ").size(9.0).strong().color(MINT));
                                ui.label(RichText::new(format!("Minty Launcher {}", info.version)).size(17.0).strong().color(TEXT));
                                ui.label(RichText::new("Можно установить сейчас или открыть страницу загрузки вручную.")
                                    .size(10.0).color(MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Пропустить версию").clicked() { self.skip_update(); }
                                if ui.small_button("Скачать вручную").clicked() { self.open_manual_update(); }
                                if ui.add_enabled(!self.update_downloading,
                                    egui::Button::new(if self.update_downloading { "Загрузка…" } else { "Обновить автоматически" })
                                        .fill(MINT).rounding(Rounding::same(9.0))).clicked() {
                                    self.start_update();
                                }
                            });
                        });
                    });
            } else if let Some(notice) = &self.update_notice {
                if notice.contains("приватный") || notice.contains("HTTP 404") {
                    ui.add_space(11.0);
                    egui::Frame::none().fill(PANEL).rounding(Rounding::same(13.0))
                        .inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Обновления").strong().color(TEXT));
                                ui.label(RichText::new("Автопроверка недоступна для приватного репозитория.")
                                    .size(10.0).color(MUTED));
                                if ui.small_button("Открыть релизы").clicked() {
                                    let _ = open::that("https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases");
                                }
                            });
                        });
                }
            }

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let launch = egui::Button::new(
                    RichText::new(if self.launch_busy { "ПОДГОТОВКА ИГРЫ…" } else { "▶   ЗАПУСТИТЬ MINECRAFT" })
                        .size(14.0).strong().color(Color32::from_rgb(15, 35, 28)))
                    .fill(MINT).min_size(egui::vec2(260.0, 52.0)).rounding(Rounding::same(13.0));
                if ui.add_enabled(!self.launch_busy && !self.install_busy, launch).clicked() { self.start_game(); }
                if ui.button("Открыть папку игры").clicked() {
                    let _ = std::fs::create_dir_all(&self.config.game_dir);
                    let _ = open::that(&self.config.game_dir);
                }
            });
            ui.add_space(7.0);
            ui.label(RichText::new("Офлайн-режим подходит для одиночной игры. Для официальных серверов потребуется лицензия Microsoft.")
                .size(10.0).color(MUTED));
        });
    }

    fn installations_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(7.0);
            ui.label(RichText::new("Версии и сборки").size(28.0).strong().color(TEXT));
            ui.label(RichText::new("Управляй версиями Minecraft и устанавливай совместимые загрузчики модов.")
                .size(13.0).color(MUTED));
            ui.add_space(17.0);
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ЗАГРУЗЧИК").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("install_loader_picker")
                                .selected_text(&self.config.selected_loader).width(140.0).show_ui(ui, |ui| {
                                    for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                        if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                            self.config.selected_profile_id.clear();
                                            self.config.selected_loader_version.clear();
                                        }
                                    }
                                });
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ВЕРСИЯ ЗАГРУЗЧИКА").size(9.0).strong().color(MUTED));
                            ui.add(egui::TextEdit::singleline(&mut self.config.selected_loader_version)
                                .desired_width(150.0).hint_text("Последняя стабильная"));
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ПОИСК ВЕРСИИ").size(9.0).strong().color(MUTED));
                            ui.add(egui::TextEdit::singleline(&mut self.version_search)
                                .desired_width(170.0).hint_text("1.21.4 / 24w14a"));
                        });
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ТИП").size(9.0).strong().color(MUTED));
                            egui::ComboBox::from_id_source("version_type_filter")
                                .selected_text(&self.version_type_filter).width(120.0).show_ui(ui, |ui| {
                                    for kind in ["Все", "Release", "Snapshot", "Old Alpha", "Old Beta"] {
                                        ui.selectable_value(&mut self.version_type_filter, kind.to_owned(), kind);
                                    }
                                });
                        });
                        if ui.button("↻").on_hover_text("Обновить список").clicked() { self.load_versions(); }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Оставь поле сборки пустым, чтобы выбрать последнюю совместимую стабильную версию.")
                        .size(10.0).color(MUTED));
                    if ui.button("Сохранить параметры сборки").clicked() { let _ = config::save(&self.config); }
                });

            ui.add_space(13.0);
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
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Найдено версий: {}", filtered.len())).size(11.0).color(MUTED));
                ui.label(RichText::new(format!("Официальный последний релиз: {}", self.latest_release))
                    .size(10.0).color(MUTED));
            });
            let mut requested_install = None;
            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(430.0).auto_shrink([false, false]).show(ui, |ui| {
                        for version in filtered {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&version.id).strong().size(13.0).color(TEXT));
                                    let kind = match version.kind.as_str() {
                                        "release" => "СТАБИЛЬНАЯ",
                                        "snapshot" => "СНИМОК",
                                        "old_alpha" => "ALPHA",
                                        "old_beta" => "BETA",
                                        _ => "ВЕРСИЯ",
                                    };
                                    ui.label(RichText::new(kind).size(9.0).strong().color(MINT));
                                });
                                if let Some(date) = &version.time {
                                    ui.label(RichText::new(date.get(..10).unwrap_or(date)).size(10.0).color(MUTED));
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let button_text = if self.config.selected_version == version.id { "Установить выбранную" } else { "Выбрать и скачать" };
                                    if ui.add_enabled(!self.install_busy && !self.launch_busy,
                                        egui::Button::new(button_text).rounding(Rounding::same(8.0))).clicked() {
                                        requested_install = Some(version.id.clone());
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            ui.separator();
                        }
                        if self.versions.is_empty() {
                            ui.label(RichText::new("Список версий ещё не загружен. Проверь соединение и нажми ↻.").color(MUTED));
                        }
                    });
                });
            if let Some(version) = requested_install { self.install_selected_version(version); }
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Папка Minecraft: {}", self.config.game_dir.display())).size(10.0).color(MUTED));
        });
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

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(7.0);
            ui.label(RichText::new("Настройки").size(28.0).strong().color(TEXT));
            ui.label(RichText::new("Настрой папки, ресурсы компьютера, профиль и обновления.")
                .size(13.0).color(MUTED));
            ui.add_space(15.0);

            egui::Frame::none().fill(PANEL).rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::same(17.0)).show(ui, |ui| {
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
            ui.add_space(4.0);
            ui.label(RichText::new(error).size(10.0).color(RED));
        }
    }
}

impl eframe::App for MintyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_events();
        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(21.0)))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("status_footer")
                    .frame(egui::Frame::none().fill(BG))
                    .show_inside(ui, |ui| self.footer(ui));
                match self.page {
                    Page::Home => self.home_page(ui),
                    Page::Installations => self.installations_page(ui),
                    Page::Skin => self.skin_page(ui),
                    Page::Journal => self.journal_page(ui),
                    Page::Settings => self.settings_page(ui),
                }
            });
        if self.auth_busy || self.launch_busy || self.install_busy || self.version_rx.is_some()
            || self.update_checking || self.update_downloading {
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

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(10, 14, 19);
    style.visuals.faint_bg_color = PANEL;
    style.visuals.selection.bg_fill = MINT_DARK;
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 68, 78);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.bg_fill = MINT_DARK;
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

use crate::{
    auth::{self, Account, AuthEvent},
    config::{self, AppConfig},
    minecraft::{self, LaunchEvent, VersionInfo},
    skin,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Rounding, Stroke};
use std::{path::PathBuf, thread, time::Duration};

const BG: Color32 = Color32::from_rgb(10, 17, 15);
const SIDEBAR: Color32 = Color32::from_rgb(13, 24, 20);
const PANEL: Color32 = Color32::from_rgb(19, 31, 26);
const PANEL_LIGHT: Color32 = Color32::from_rgb(28, 44, 37);
const MINT: Color32 = Color32::from_rgb(113, 235, 179);
const MINT_DARK: Color32 = Color32::from_rgb(43, 143, 105);
const TEXT: Color32 = Color32::from_rgb(236, 246, 240);
const MUTED: Color32 = Color32::from_rgb(143, 165, 153);
const RED: Color32 = Color32::from_rgb(237, 110, 110);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Page {
    Home,
    Installations,
    Skin,
    Journal,
    Settings,
}

pub struct MintyApp {
    config: AppConfig,
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
        let mut app = Self {
            config,
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

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .exact_width(226.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::same(17.0)))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("✳").size(32.0).color(MINT));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MINTY").size(20.0).strong().color(TEXT));
                            ui.label(RichText::new("MINECRAFT LAUNCHER").size(9.0).strong().color(MUTED));
                        });
                    });
                    ui.add_space(28.0);
                    self.nav_button(ui, "⌂", "Главная", Page::Home);
                    self.nav_button(ui, "▣", "Установки", Page::Installations);
                    self.nav_button(ui, "✎", "Редактор скина", Page::Skin);
                    self.nav_button(ui, "≡", "Журнал", Page::Journal);
                    self.nav_button(ui, "⚙", "Настройки", Page::Settings);
                    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("v{} · Windows x64", env!("CARGO_PKG_VERSION"))).size(10.0).color(MUTED));
                        ui.label(RichText::new("MINTY PROJECT").size(9.0).color(MUTED));
                    });
                });
            });
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, icon: &str, label: &str, page: Page) {
        let selected = self.page == page;
        let fill = if selected { PANEL_LIGHT } else { Color32::TRANSPARENT };
        let response = ui.add_sized(
            [190.0, 43.0],
            egui::Button::new(
                RichText::new(format!("{icon}    {label}"))
                    .size(13.0)
                    .color(if selected { TEXT } else { MUTED }),
            )
            .fill(fill)
            .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
            .rounding(Rounding::same(10.0)),
        );
        if response.clicked() {
            self.page = page;
        }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(response.rect.left_top(), egui::vec2(3.0, response.rect.height())),
                Rounding::same(2.0),
                MINT,
            );
        }
        ui.add_space(5.0);
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("MINTY LAUNCHER  /  JAVA EDITION").size(10.0).strong().color(MINT));
            ui.add_space(8.0);
            ui.label(RichText::new("Привет, игрок 👋").size(29.0).strong().color(TEXT));
            ui.label(RichText::new("Твоя игра начинается здесь. Выбери версию и запускай.").size(13.0).color(MUTED));
            ui.add_space(22.0);

            egui::Frame::none()
                .fill(PANEL)
                .rounding(Rounding::same(18.0))
                .inner_margin(egui::Margin::same(22.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("ТЕКУЩАЯ УСТАНОВКА").size(10.0).strong().color(MUTED));
                            ui.add_space(6.0);
                            ui.label(RichText::new(if self.config.selected_version == "latest-release" { "Последний релиз" } else { &self.config.selected_version }).size(24.0).strong().color(TEXT));
                            ui.label(RichText::new(format!("Minecraft · Java Edition · {}", self.config.selected_loader)).size(12.0).color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("✦").size(58.0).color(MINT_DARK));
                        });
                    });
                    ui.add_space(17.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Загрузчик").color(TEXT).size(12.0));
                        egui::ComboBox::from_id_source("home_loader_picker")
                            .selected_text(&self.config.selected_loader).width(145.0)
                            .show_ui(ui, |ui| {
                                for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                    if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                        self.config.selected_profile_id.clear();
                                        self.config.selected_loader_version.clear();
                                        let _ = config::save(&self.config);
                                    }
                                }
                            });
                        ui.label(RichText::new("Версия игры").color(TEXT).size(12.0));
                        egui::ComboBox::from_id_source("version_picker")
                            .selected_text(if self.config.selected_version.is_empty() { "Загрузка…" } else { &self.config.selected_version })
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if !self.latest_release.is_empty() {
                                    ui.selectable_value(&mut self.config.selected_version, self.latest_release.clone(), format!("{} (последняя)", self.latest_release));
                                }
                                for version in self.versions.iter().take(150) {
                                    ui.selectable_value(&mut self.config.selected_version, version.id.clone(), version.id.clone());
                                }
                            });
                        if ui.button("↻").on_hover_text("Обновить список версий").clicked() {
                            self.load_versions();
                        }
                    });
                });

            ui.add_space(15.0);
            ui.columns(2, |columns| {
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0)).inner_margin(egui::Margin::same(18.0)).show(&mut columns[0], |ui| {
                    ui.label(RichText::new("АККАУНТ").size(10.0).strong().color(MUTED));
                    ui.add_space(8.0);
                    if let Some(account_name) = self.account.as_ref().map(|account| account.name.clone()) {
                        ui.label(RichText::new(&account_name).size(18.0).strong().color(TEXT));
                        ui.label(RichText::new("Microsoft · лицензия проверена").size(10.0).color(MINT));
                        ui.add_space(7.0);
                        if ui.small_button("Выйти из аккаунта").clicked() {
                            auth::sign_out();
                            self.account = None;
                            self.config.last_account_name = None;
                            self.config.last_account_uuid = None;
                            self.config.last_account_xuid = None;
                            let _ = config::save(&self.config);
                            self.log("Выполнен выход из Microsoft-аккаунта");
                        }
                    } else {
                        ui.label(RichText::new("Офлайн-профиль").size(17.0).strong().color(TEXT));
                        ui.label(RichText::new("Для серверов потребуется Microsoft-вход").size(10.0).color(MUTED));
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.config.offline_username).desired_width(112.0).hint_text("Ник игрока"));
                            if ui.small_button("Войти ↗").clicked() {
                                self.begin_login();
                            }
                        });
                    }
                });
                egui::Frame::none().fill(PANEL).rounding(Rounding::same(14.0)).inner_margin(egui::Margin::same(18.0)).show(&mut columns[1], |ui| {
                    ui.label(RichText::new("ПРОИЗВОДИТЕЛЬНОСТЬ").size(10.0).strong().color(MUTED));
                    ui.add_space(8.0);
                    let mut weak = self.config.weak_pc_mode;
                    if ui.checkbox(&mut weak, RichText::new("Слабый ПК").strong().color(TEXT)).changed() {
                        self.config.weak_pc_mode = weak;
                        let _ = config::save(&self.config);
                        self.log(if weak { "Включён профиль «Слабый ПК»" } else { "Выключен профиль «Слабый ПК»; исходные настройки графики восстановятся при следующем запуске" });
                    }
                    ui.label(RichText::new(if weak { "Более низкая дальность прорисовки, эффекты и лимит памяти" } else { "Обычные настройки графики" }).size(10.0).color(MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("Память Java: {} GB", self.config.max_memory_gb)).size(12.0).color(TEXT));
                    ui.add(egui::Slider::new(&mut self.config.max_memory_gb, 2..=16).show_value(false));
                });
            });

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let button = egui::Button::new(RichText::new(if self.launch_busy { "ПОДГОТОВКА…" } else { "▶   ЗАПУСТИТЬ MINECRAFT" }).size(14.0).strong().color(Color32::from_rgb(8, 29, 19)))
                    .fill(MINT)
                    .min_size(egui::vec2(260.0, 52.0))
                    .rounding(Rounding::same(12.0));
                if ui.add_enabled(!self.launch_busy && !self.install_busy, button).clicked() {
                    self.start_game();
                }
                ui.add_space(8.0);
                if ui.button(RichText::new("Открыть папку игры").color(TEXT)).clicked() {
                    let _ = std::fs::create_dir_all(&self.config.game_dir);
                    let _ = open::that(&self.config.game_dir);
                }
            });
            ui.add_space(9.0);
            ui.label(RichText::new("Офлайн-режим — для одиночной игры. Для авторизованных серверов используй лицензионный Microsoft-аккаунт.").size(10.0).color(MUTED));
        });
    }

    fn installations_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Установки").size(27.0).strong().color(TEXT));
        ui.label(RichText::new("Выбери версию Minecraft и загрузчик, затем установи профиль.").size(13.0).color(MUTED));
        ui.add_space(19.0);
        let mut requested_install: Option<String> = None;
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0))
            .inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                let mut settings_changed = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Загрузчик").color(TEXT));
                    egui::ComboBox::from_id_source("install_loader_picker")
                        .selected_text(&self.config.selected_loader).width(140.0)
                        .show_ui(ui, |ui| {
                            for loader in ["Vanilla", "Forge", "NeoForge", "Fabric", "Quilt"] {
                                if ui.selectable_value(&mut self.config.selected_loader, loader.to_owned(), loader).changed() {
                                    self.config.selected_profile_id.clear();
                                    self.config.selected_loader_version.clear();
                                    settings_changed = true;
                                }
                            }
                        });
                    ui.label(RichText::new("Сборка").color(TEXT));
                    if ui.add(egui::TextEdit::singleline(&mut self.config.selected_loader_version)
                        .desired_width(130.0).hint_text("Последняя стабильная")).changed() {
                        self.config.selected_profile_id.clear();
                        settings_changed = true;
                    }
                });
                ui.label(RichText::new("Оставь сборку пустой — будет выбрана последняя совместимая стабильная версия.").size(10.0).color(MUTED));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Поиск").color(TEXT));
                    ui.add(egui::TextEdit::singleline(&mut self.version_search)
                        .desired_width(220.0).hint_text("Например, 1.21.4 или 24w14a"));
                    ui.label(RichText::new("Тип").color(TEXT));
                    egui::ComboBox::from_id_source("version_type_filter")
                        .selected_text(&self.version_type_filter).width(120.0)
                        .show_ui(ui, |ui| {
                            for kind in ["Все", "Release", "Snapshot", "Old Alpha", "Old Beta"] {
                                ui.selectable_value(&mut self.version_type_filter, kind.to_owned(), kind);
                            }
                        });
                    if ui.button("↻").on_hover_text("Обновить весь список версий").clicked() {
                        self.load_versions();
                    }
                });
                if settings_changed { let _ = config::save(&self.config); }
                ui.add_space(12.0);
                let filtered: Vec<VersionInfo> = self.versions.iter()
                    .filter(|v| {
                        let query = self.version_search.is_empty()
                            || v.id.to_ascii_lowercase().contains(&self.version_search.to_ascii_lowercase());
                        let kind = match self.version_type_filter.as_str() {
                            "Release" => v.kind == "release",
                            "Snapshot" => v.kind == "snapshot",
                            "Old Alpha" => v.kind == "old_alpha",
                            "Old Beta" => v.kind == "old_beta",
                            _ => true,
                        };
                        query && kind
                    }).cloned().collect();
                ui.label(RichText::new(format!("Найдено версий: {}", filtered.len())).size(10.0).color(MUTED));
                egui::ScrollArea::vertical().max_height(370.0).auto_shrink([false, false]).show(ui, |ui| {
                    for version in filtered {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&version.id).strong().color(TEXT).size(12.0));
                            let kind = match version.kind.as_str() {
                                "release" => "RELEASE", "snapshot" => "SNAPSHOT",
                                "old_alpha" => "ALPHA", "old_beta" => "BETA", _ => "VERSION",
                            };
                            ui.label(RichText::new(kind).size(9.0).strong().color(MINT));
                            if let Some(date) = &version.time {
                                ui.label(RichText::new(date.get(..10).unwrap_or(date)).size(10.0).color(MUTED));
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let button = if self.install_busy && self.config.selected_version == version.id {
                                    "Установка…"
                                } else if self.config.selected_version == version.id { "Установить снова" } else { "Скачать" };
                                if ui.add_enabled(!self.install_busy && !self.launch_busy, egui::Button::new(button)).clicked() {
                                    requested_install = Some(version.id.clone());
                                }
                            });
                        });
                        ui.add_space(5.0);
                        ui.separator();
                    }
                });
                if self.versions.is_empty() {
                    ui.label(RichText::new("Список версий пока не загружен. Проверь интернет и обнови список.").color(MUTED));
                }
            });
        if let Some(version_id) = requested_install { self.install_selected_version(version_id); }
        ui.add_space(12.0);
        ui.label(RichText::new(format!("Папка игры: {}", self.config.game_dir.display())).size(10.0).color(MUTED));
        ui.label(RichText::new("Загрузчики устанавливаются только для совместимых версий Minecraft.").size(10.0).color(MUTED));
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
        ui.add_space(7.0);
        ui.label(RichText::new("Настройки").size(27.0).strong().color(TEXT));
        ui.label(RichText::new("Каталог игры, память, оптимизация и авторизация.").size(13.0).color(MUTED));
        ui.add_space(15.0);
        egui::Frame::none().fill(PANEL).rounding(Rounding::same(15.0)).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
            ui.set_max_width(760.0);
            field_label(ui, "ПАПКА УСТАНОВКИ MINECRAFT");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.directory_input).desired_width(490.0));
                if ui.button("Обзор…").clicked() { self.choose_game_directory(); }
                if ui.button("Открыть").clicked() {
                    let _ = std::fs::create_dir_all(PathBuf::from(self.directory_input.trim()));
                    let _ = open::that(self.directory_input.trim());
                }
            });
            ui.label(RichText::new("Сюда будут устанавливаться версии, библиотеки, ресурсы и папки Minecraft.").size(10.0).color(MUTED));
            ui.add_space(14.0);
            field_label(ui, "MICROSOFT OAUTH CLIENT ID");
            ui.add(egui::TextEdit::singleline(&mut self.config.microsoft_client_id).desired_width(570.0).hint_text("Application (client) ID из Microsoft Entra"));
            ui.label(RichText::new("Нужен для официального входа. Настрой Public client / Mobile & desktop и Device Code Flow. Client Secret в лаунчер не добавляй.").size(10.0).color(MUTED));
            ui.add_space(14.0);
            field_label(ui, "JAVA EXECUTABLE (НЕОБЯЗАТЕЛЬНО)");
            ui.add(egui::TextEdit::singleline(&mut self.config.java_path).desired_width(570.0).hint_text("Пусто — Minty установит подходящую Java самостоятельно"));
            ui.add_space(14.0);
            field_label(ui, "ОПЕРАТИВНАЯ ПАМЯТЬ JAVA");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.config.max_memory_gb, 2..=16).suffix(" GB"));
                ui.label(RichText::new("От 2 до 16 GB").size(10.0).color(MUTED));
            });
            ui.add_space(12.0);
            field_label(ui, "ОПТИМИЗАЦИЯ");
            ui.checkbox(&mut self.config.weak_pc_mode, RichText::new("Слабый ПК — включить экономию ресурсов").strong().color(TEXT));
            ui.label(RichText::new("При следующем запуске Minty уменьшит дальность прорисовки, эффекты и выделение памяти Java. Оригинальный options.txt будет сохранён и восстановлен после выключения режима.").size(10.0).color(MUTED));
            ui.add_space(18.0);
            if ui.add_sized([190.0, 42.0], egui::Button::new(RichText::new("СОХРАНИТЬ НАСТРОЙКИ").strong().color(Color32::from_rgb(8, 29, 19))).fill(MINT).rounding(Rounding::same(10.0))).clicked() {
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
        if self.auth_busy || self.launch_busy || self.install_busy || self.version_rx.is_some() {
            let interval = if self.config.weak_pc_mode { 500 } else { 150 };
            ctx.request_repaint_after(Duration::from_millis(interval));
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
    style.visuals.extreme_bg_color = Color32::from_rgb(7, 12, 10);
    style.visuals.faint_bg_color = PANEL;
    style.visuals.selection.bg_fill = MINT_DARK;
    style.visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 66, 54);
    style.visuals.widgets.active.bg_fill = MINT_DARK;
    style.visuals.override_text_color = Some(TEXT);
    style.spacing.item_spacing = egui::vec2(9.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
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

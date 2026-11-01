#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bedrock;
mod mods;
mod auth;
mod config;
mod minecraft;
mod runtime;
mod skin;
mod system_info;
mod updater;

use std::sync::Arc;

const APP_MUTEX: &str = "Local\\MintyLauncherAppMutex";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--apply-update") {
        if let Err(error) = updater::run_helper(&args) {
            show_message_box(&format!("Не удалось применить обновление Minty Launcher:\n\n{error}"), true);
        }
        return;
    }

    std::panic::set_hook(Box::new(|info| {
        let message = format!("Minty Launcher неожиданно завершился: {info}");
        let _ = config::append_log(&message);
        show_message_box(&message, true);
    }));

    #[cfg(windows)]
    let _instance_mutex = match acquire_instance_mutex() {
        Ok(Some(handle)) => handle,
        Ok(None) => return,
        Err(error) => {
            show_message_box(&format!("Не удалось проверить запущенные копии лаунчера.\n\n{error}"), true);
            return;
        }
    };

    if let Err(error) = run() {
        let message = format!("Minty Launcher не смог запуститься.\n\n{error}\n\nПодробности записаны в журнал Minty Launcher.");
        let _ = config::append_log(&message);
        show_message_box(&message, true);
    }
}

fn run() -> Result<(), String> {
    let icon_image = image::load_from_memory(include_bytes!("../assets/minty-launcher-icon.png"))
        .map_err(|error| format!("Не удалось загрузить встроенную иконку: {error}"))?
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (icon_width, icon_height) = icon_image.dimensions();
    let window_icon = Arc::new(eframe::egui::IconData {
        rgba: icon_image.into_raw(),
        width: icon_width,
        height: icon_height,
    });

    eframe::run_native(
        "Minty Launcher",
        native_options(window_icon, eframe::Renderer::Glow),
        Box::new(|cc| Ok(Box::new(app::MintyApp::new(cc)))),
    )
    .map_err(|error| {
        format!(
            "Не удалось открыть окно Minty Launcher: {error}.\\n\\nПроверь, что установлен драйвер видеокарты и поддерживается OpenGL 3.2 или новее."
        )
    })
}

fn native_options(icon: Arc<eframe::egui::IconData>, renderer: eframe::Renderer) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([900.0, 650.0])
            .with_title("Minty Launcher")
            .with_icon(icon),
        renderer,
        ..Default::default()
    }
}

#[cfg(windows)]
struct InstanceMutex { _handle: *mut std::ffi::c_void }

#[cfg(windows)]
fn acquire_instance_mutex() -> Result<Option<InstanceMutex>, String> {
    use std::ptr::null_mut;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attributes: *mut std::ffi::c_void, initial_owner: i32, name: *const u16) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    let name: Vec<u16> = APP_MUTEX.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { CreateMutexW(null_mut(), 0, name.as_ptr()) };
    if handle.is_null() { return Err(format!("Windows CreateMutexW: {}", unsafe { GetLastError() })); }
    if unsafe { GetLastError() } == 183 {
        unsafe { CloseHandle(handle); }
        show_message_box("Minty Launcher уже запущен. Переключись на открытое окно.", false);
        return Ok(None);
    }
    Ok(Some(InstanceMutex { _handle: handle }))
}

#[cfg(windows)]
fn show_message_box(message: &str, error: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let caption: Vec<u16> = "Minty Launcher".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), if error { 0x10 } else { 0x40 }); }
}

#[cfg(not(windows))]
fn show_message_box(message: &str, _error: bool) { eprintln!("{message}"); }

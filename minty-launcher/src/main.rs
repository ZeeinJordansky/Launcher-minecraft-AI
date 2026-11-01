#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod auth;
mod config;
mod minecraft;
mod runtime;
mod skin;

use std::sync::Arc;

fn main() -> eframe::Result<()> {
    let icon_image = image::load_from_memory(include_bytes!("../assets/minty-launcher-icon.png"))
        .expect("Embedded Minty Launcher icon must be a valid PNG")
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (icon_width, icon_height) = icon_image.dimensions();
    let window_icon = Arc::new(eframe::egui::IconData {
        rgba: icon_image.into_raw(),
        width: icon_width,
        height: icon_height,
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([900.0, 650.0])
            .with_title("Minty Launcher")
            .with_icon(window_icon),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Minty Launcher",
        options,
        Box::new(|cc| Ok(Box::new(app::MintyApp::new(cc)))),
    )
}

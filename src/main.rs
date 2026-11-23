#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod browser;
mod devices;
mod import;
mod model;
mod proxy;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);

    let mut vp = egui::ViewportBuilder::default()
        .with_title("Nebula Sessions")
        .with_inner_size([1369.0, 681.0])
        .with_min_inner_size([1100.0, 560.0])
        .with_decorations(false);
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native("Nebula Sessions", options, Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shot)))))
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod autostart;
mod config;
mod media;
mod process;

use parking_lot::Mutex;
use std::sync::Arc;
use tracing::info;
#[cfg(not(target_os = "windows"))]
use tracing::warn;

use crate::config::{AppConfig, AppStatus};

#[cfg(target_os = "windows")]
mod probe;

fn window_icon() -> eframe::egui::IconData {
    const ICON_PNG: &[u8] = include_bytes!("../assets/nsound-64.png");

    match image::load_from_memory(ICON_PNG) {
        Ok(image) => {
            let image = image.to_rgba8();
            let (width, height) = image.dimensions();
            eframe::egui::IconData {
                rgba: image.into_raw(),
                width,
                height,
            }
        }
        Err(err) => {
            tracing::warn!("cannot decode the window icon: {err}");
            eframe::egui::IconData {
                rgba: vec![0, 0, 0, 0],
                width: 1,
                height: 1,
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Diagnostics: print which audio sessions are loud, without touching media.
    #[cfg(target_os = "windows")]
    if let Some(index) = std::env::args().position(|arg| arg == "--probe") {
        let _ = index;
        probe::run();
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    if std::env::args().any(|arg| arg == "--unmute-mic") {
        probe::unmute_microphone();
        return Ok(());
    }

    if let Some(request) = std::env::args().find(|arg| {
        arg == "--autostart-enable" || arg == "--autostart-disable"
    }) {
        let enable = request == "--autostart-enable";
        match autostart::set_enabled(enable) {
            Ok(()) => println!("autostart enabled: {}", autostart::is_enabled()),
            Err(err) => println!("cannot change autostart: {err}"),
        }
        return Ok(());
    }

    let config = Arc::new(Mutex::new(AppConfig::load()));
    let status = Arc::new(Mutex::new(AppStatus::default()));

    info!("nSound starting");

    // Background worker: watches Discord/Teams processes, reads the level of
    // the audio they are playing (WASAPI sessions) and drives media transport.
    #[cfg(target_os = "windows")]
    audio::AudioMonitor::start_background_loop(Arc::clone(&config), Arc::clone(&status));

    #[cfg(not(target_os = "windows"))]
    {
        warn!("Communicator audio monitoring is implemented for Windows only");
        status.lock().status_message =
            "Not supported on this platform yet (Windows only)".to_string();
    }

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([460.0, 700.0])
            .with_min_inner_size([440.0, 620.0])
            .with_icon(window_icon())
            .with_title("nSound"),
        ..Default::default()
    };

    eframe::run_native(
        "nSound",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::NSoundApp::new(cc, config, status)))),
    )
}

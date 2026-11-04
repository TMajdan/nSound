use eframe::egui;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use crate::audio::vad::display_db;
use crate::config::{AppConfig, AppStatus, CommunicatorConfig};
use crate::media::MediaController;
use crate::autostart;

const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(16, 17, 21);
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(25, 27, 33);
const CARD_BORDER: egui::Color32 = egui::Color32::from_rgb(40, 43, 52);
const TRACK_BG: egui::Color32 = egui::Color32::from_rgb(35, 38, 46);
const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(148, 153, 166);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(88, 101, 242);
const OK_GREEN: egui::Color32 = egui::Color32::from_rgb(80, 200, 120);
const WARN_YELLOW: egui::Color32 = egui::Color32::from_rgb(235, 190, 80);
const ALERT_RED: egui::Color32 = egui::Color32::from_rgb(225, 95, 95);

/// Level (RMS) that fills the meter bar completely.
const METER_FULL_SCALE: f32 = 0.15;

pub struct NSoundApp {
    config: Arc<Mutex<AppConfig>>,
    status: Arc<Mutex<AppStatus>>,
    start_with_windows: bool,
    autostart_error: Option<String>,
}

impl NSoundApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Arc<Mutex<AppConfig>>,
        status: Arc<Mutex<AppStatus>>,
    ) -> Self {
        setup_style(&cc.egui_ctx);
        Self {
            config,
            status,
            start_with_windows: autostart::is_enabled(),
            autostart_error: None,
        }
    }
}

impl eframe::App for NSoundApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(40));

        let mut cfg = self.config.lock();
        let stat = self.status.lock().clone();
        let mut config_changed = false;

        header(ctx, &mut cfg, &mut config_changed);
        status_bar(ctx, &cfg, &stat);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                card(ui, |ui| {
                    section_title(ui, "COMMUNICATORS");
                    for comm in cfg.communicators.iter_mut() {
                        let running = stat
                            .communicator_running
                            .get(&comm.id)
                            .copied()
                            .unwrap_or(false);
                        communicator_row(ui, comm, running, &mut config_changed);
                    }
                });

                ui.add_space(10.0);

                card(ui, |ui| {
                    section_title(ui, "VOICE DETECTION");
                    level_meter(ui, &stat, cfg.rms_threshold);
                    ui.add_space(6.0);

                    if slider_row(
                        ui,
                        "Sensitivity threshold",
                        format!("{:.3}", cfg.rms_threshold),
                        &mut cfg.rms_threshold,
                        0.002..=0.150,
                        true,
                    ) {
                        config_changed = true;
                    }

                    if slider_row(
                        ui,
                        "Pause trigger delay (attack)",
                        format!("{} ms", cfg.attack_ms),
                        &mut cfg.attack_ms,
                        50..=1000,
                        false,
                    ) {
                        config_changed = true;
                    }

                    if slider_row(
                        ui,
                        "Resume hold time",
                        format!("{} ms", cfg.hold_time_ms),
                        &mut cfg.hold_time_ms,
                        300..=3000,
                        false,
                    ) {
                        config_changed = true;
                    }

                    ui.add_space(4.0);

                    if option_row(
                        ui,
                        &mut cfg.auto_resume,
                        "Resume playback when the talking stops",
                    ) {
                        config_changed = true;
                    }
                });

                ui.add_space(10.0);

                card(ui, |ui| {
                    section_title(ui, "APPLICATION");

                    let mut start_with_windows = self.start_with_windows;
                    if option_row(ui, &mut start_with_windows, "Start with Windows") {
                        match autostart::set_enabled(start_with_windows) {
                            Ok(()) => {
                                self.start_with_windows = start_with_windows;
                                self.autostart_error = None;
                            }
                            Err(err) => {
                                tracing::warn!("Cannot change autostart: {err}");
                                self.autostart_error = Some(err);
                            }
                        }
                    }

                    if let Some(err) = &self.autostart_error {
                        ui.label(
                            egui::RichText::new(format!("Autostart error: {err}"))
                                .size(11.0)
                                .color(ALERT_RED),
                        );
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Test play/pause").clicked() {
                            // Off the UI thread: the WinRT call blocks until the
                            // media session acknowledges the command.
                            std::thread::spawn(|| MediaController::new().toggle_play_pause());
                        }
                        if ui.button("Test pause").clicked() {
                            std::thread::spawn(|| MediaController::new().pause_media());
                        }
                    });
                });
            });

        if config_changed {
            let _ = cfg.save();
        }
    }
}

// --- Layout helpers -------------------------------------------------------

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = CARD_BG;
    visuals.override_text_color = Some(egui::Color32::from_rgb(226, 229, 236));
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.rounding = 8.0.into();
    }
    visuals.widgets.noninteractive.bg_fill = CARD_BG;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, CARD_BORDER);
    visuals.widgets.inactive.bg_fill = TRACK_BG;
    visuals.widgets.inactive.weak_bg_fill = TRACK_BG;
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 49, 60);
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(45, 49, 60);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.slider_width = 150.0;
    ctx.set_style(style);
}

fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD_BG)
        .rounding(12.0)
        .stroke(egui::Stroke::new(1.0_f32, CARD_BORDER))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(10.5)
            .strong()
            .color(TEXT_DIM),
    );
    ui.add_space(6.0);
}

fn header(ctx: &egui::Context, cfg: &mut AppConfig, config_changed: &mut bool) {
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::none()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("nSound").size(19.0).strong());
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("pause on voice")
                        .size(11.0)
                        .color(TEXT_DIM),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if toggle(ui, &mut cfg.master_enabled).changed() {
                        *config_changed = true;
                    }
                    ui.label(
                        egui::RichText::new(if cfg.master_enabled { "ACTIVE" } else { "OFF" })
                            .size(11.0)
                            .strong()
                            .color(if cfg.master_enabled { OK_GREEN } else { TEXT_DIM }),
                    );
                });
            });
        });
}

fn status_bar(ctx: &egui::Context, cfg: &AppConfig, stat: &AppStatus) {
    let color = if !cfg.master_enabled {
        TEXT_DIM
    } else if stat.is_speaking {
        OK_GREEN
    } else if stat.is_monitoring {
        ACCENT
    } else {
        WARN_YELLOW
    };

    egui::TopBottomPanel::bottom("status")
        .frame(
            egui::Frame::none()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::symmetric(14.0, 9.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(stat.status_message.as_str())
                        .size(11.5)
                        .color(color),
                );
            });
        });
}

fn communicator_row(
    ui: &mut egui::Ui,
    comm: &mut CommunicatorConfig,
    running: bool,
    config_changed: &mut bool,
) {
    ui.horizontal(|ui| {
        badge(ui, comm);

        ui.vertical(|ui| {
            ui.label(egui::RichText::new(&comm.name).size(13.5).strong());
            ui.horizontal(|ui| {
                let (dot, _) =
                    ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    dot.center(),
                    3.0,
                    if running { OK_GREEN } else { TEXT_DIM },
                );
                ui.label(
                    egui::RichText::new(if running {
                        "process running"
                    } else {
                        "process not running"
                    })
                    .size(10.5)
                    .color(TEXT_DIM),
                );
            });
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if toggle(ui, &mut comm.enabled).changed() {
                *config_changed = true;
            }
        });
    });
}

fn badge(ui: &mut egui::Ui, comm: &CommunicatorConfig) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
    let [r, g, b] = comm.brand_color;
    ui.painter()
        .rect_filled(rect, 8.0, egui::Color32::from_rgb(r, g, b));

    let label = if comm.short_label.is_empty() {
        comm.name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    } else {
        comm.short_label.clone()
    };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
}

fn level_meter(ui: &mut egui::Ui, stat: &AppStatus, threshold: f32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Live level").size(11.0).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{:.1} dB", display_db(stat.current_db)))
                    .size(11.0)
                    .strong(),
            );
        });
    });

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 12.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, TRACK_BG);

    let level = (stat.current_rms / METER_FULL_SCALE).clamp(0.0, 1.0);
    if level > 0.0 {
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2((rect.width() * level).max(4.0), rect.height()),
        );
        painter.rect_filled(
            bar,
            6.0,
            if stat.is_speaking { OK_GREEN } else { ACCENT },
        );
    }

    let threshold_x = rect.left()
        + rect.width() * (threshold / METER_FULL_SCALE).clamp(0.0, 1.0);
    painter.line_segment(
        [
            egui::pos2(threshold_x, rect.top() - 3.0),
            egui::pos2(threshold_x, rect.bottom() + 3.0),
        ],
        egui::Stroke::new(2.0_f32, WARN_YELLOW),
    );
}

fn slider_row<T>(
    ui: &mut egui::Ui,
    label: &str,
    value_text: String,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    logarithmic: bool,
) -> bool
where
    T: egui::emath::Numeric,
{
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.0).color(TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value_text).size(11.0).strong());
        });
    });
    ui.add(
        egui::Slider::new(value, range)
            .show_value(false)
            .logarithmic(logarithmic),
    )
    .changed()
}

fn option_row(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    ui.checkbox(value, egui::RichText::new(label).size(11.5))
        .changed()
}

/// Compact pill switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let animation = ui.ctx().animate_bool(response.id, *on);
    let painter = ui.painter();
    let track = if *on {
        ACCENT
    } else {
        egui::Color32::from_rgb(58, 62, 72)
    };
    painter.rect_filled(rect, size.y * 0.5, track);

    let radius = size.y * 0.5 - 3.0;
    let center_x = egui::lerp(
        (rect.left() + size.y * 0.5)..=(rect.right() - size.y * 0.5),
        animation,
    );
    painter.circle_filled(
        egui::pos2(center_x, rect.center().y),
        radius,
        egui::Color32::WHITE,
    );

    response
}

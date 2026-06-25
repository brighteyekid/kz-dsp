//! `iem-ui` — egui GUI client for the iem-dspd daemon.
//!
//! # Architecture
//!
//! The UI maintains a local copy of `DspConfig`. Any edit triggers an async
//! IPC `SetConfig` call via a tokio runtime running on a background thread.
//! The UI never blocks on network I/O.

mod ipc;
mod widgets;

use std::sync::{Arc, Mutex};

use eframe::{egui, NativeOptions};
use egui::{
    Color32, FontId, RichText, Rounding, Stroke, Vec2,
};
use iem_common::{DspConfig, FilterType, SpatialMode};
use tokio::runtime::Runtime;
use tracing::warn;



use ipc::IpcClient;

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct IemApp {
    /// Local copy of the DSP config being edited.
    config: DspConfig,
    /// Connection to daemon; `None` if daemon not running.
    client: Option<Arc<IpcClient>>,
    /// Tokio runtime for IPC (runs on a dedicated OS thread).
    rt: Arc<Runtime>,
    /// Pending async IPC result (error string or None).
    status: Arc<Mutex<Option<String>>>,
    /// Which band is currently selected for detail editing.
    selected_band: usize,
}

impl IemApp {
    fn new(cc: &eframe::CreationContext) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_visuals(&cc.egui_ctx);

        let rt = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .thread_name("iem-ipc")
                .enable_all()
                .build()
                .expect("tokio runtime"),
        );

        let status: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // Try to connect to daemon and load current config.
        let (client, config) = {
            let rt_ref = Arc::clone(&rt);
            let status_ref = Arc::clone(&status);
            match rt_ref.block_on(async {
                let c = IpcClient::connect().await?;
                let cfg = c.get_config().await?;
                anyhow::Ok((c, cfg))
            }) {
                Ok((c, cfg)) => (Some(Arc::new(c)), cfg),
                Err(e) => {
                    warn!("Could not connect to daemon: {e}. Showing defaults.");
                    *status_ref.lock().unwrap() =
                        Some(format!("⚠ Daemon offline: {e}"));
                    (None, DspConfig::kz_castor_default())
                }
            }
        };

        Self { config, client, rt, status, selected_band: 0 }
    }

    /// Push the current config to the daemon asynchronously.
    fn push_config(&self) {
        let Some(client) = self.client.clone() else { return };
        let cfg = self.config.clone();
        let status = Arc::clone(&self.status);
        self.rt.spawn(async move {
            if let Err(e) = client.set_config(&cfg).await {
                *status.lock().unwrap() = Some(format!("IPC error: {e}"));
            } else {
                *status.lock().unwrap() = None;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Fonts & visuals
// ---------------------------------------------------------------------------

fn setup_fonts(ctx: &egui::Context) {
    let fonts = egui::FontDefinitions::default();
    ctx.set_fonts(fonts);
}

fn setup_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    // Background: deep charcoal
    visuals.panel_fill          = Color32::from_rgb(0x0E, 0x0F, 0x14);
    visuals.window_fill         = Color32::from_rgb(0x14, 0x15, 0x1E);
    visuals.extreme_bg_color    = Color32::from_rgb(0x08, 0x09, 0x0D);
    visuals.faint_bg_color      = Color32::from_rgb(0x16, 0x18, 0x24);

    // Accent: electric teal
    visuals.selection.bg_fill   = Color32::from_rgb(0x1A, 0xA8, 0x8E);
    visuals.hyperlink_color     = Color32::from_rgb(0x5E, 0xEA, 0xD4);

    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0x1C, 0x1E, 0x2A);
    visuals.widgets.inactive.bg_fill       = Color32::from_rgb(0x22, 0x25, 0x33);
    visuals.widgets.hovered.bg_fill        = Color32::from_rgb(0x2A, 0x2D, 0x42);
    visuals.widgets.active.bg_fill         = Color32::from_rgb(0x1A, 0xA8, 0x8E);

    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0x8A, 0x8D, 0xA8));
    visuals.widgets.inactive.fg_stroke       = Stroke::new(1.0, Color32::from_rgb(0xC0, 0xC4, 0xD8));
    visuals.widgets.hovered.fg_stroke        = Stroke::new(1.5, Color32::from_rgb(0xE0, 0xE4, 0xFF));
    visuals.widgets.active.fg_stroke         = Stroke::new(2.0, Color32::WHITE);

    visuals.widgets.noninteractive.rounding = Rounding::same(6.0);
    visuals.widgets.inactive.rounding       = Rounding::same(6.0);
    visuals.widgets.hovered.rounding        = Rounding::same(6.0);
    visuals.widgets.active.rounding         = Rounding::same(6.0);
    visuals.window_rounding                 = Rounding::same(10.0);

    ctx.set_visuals(visuals);

    // Spacing
    style.spacing.item_spacing   = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Main UI render
// ---------------------------------------------------------------------------

impl eframe::App for IemApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            render_header(ui, self);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            render_status_bar(ui, self);
        });

        egui::SidePanel::left("band_list")
            .min_width(160.0)
            .resizable(false)
            .show(ctx, |ui| {
                render_band_list(ui, self);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            render_main_panel(ui, self);
        });
    }
}

fn render_header(ui: &mut egui::Ui, app: &mut IemApp) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("⟁  KZ Castor DSP")
                .font(FontId::proportional(20.0))
                .color(Color32::from_rgb(0x5E, 0xEA, 0xD4))
                .strong(),
        );
        ui.separator();
        ui.label(
            RichText::new("10-Band PEQ + BS2B Crossfeed")
                .font(FontId::proportional(13.0))
                .color(Color32::from_rgb(0x8A, 0x8D, 0xA8)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("💾  Save & Apply").clicked() {
                app.push_config();
            }
            if ui.button("⟳  Reset Defaults").clicked() {
                app.config = DspConfig::kz_castor_default();
                app.push_config();
            }
        });
    });
}

fn render_status_bar(ui: &mut egui::Ui, app: &IemApp) {
    ui.horizontal(|ui| {
        let daemon_status = if app.client.is_some() {
            RichText::new("● Daemon connected").color(Color32::from_rgb(0x4E, 0xD8, 0x8A))
        } else {
            RichText::new("○ Daemon offline").color(Color32::from_rgb(0xE5, 0x55, 0x55))
        };
        ui.label(daemon_status.font(FontId::monospace(11.0)));

        if let Some(msg) = app.status.lock().unwrap().as_deref() {
            ui.separator();
            ui.label(
                RichText::new(msg)
                    .font(FontId::monospace(11.0))
                    .color(Color32::from_rgb(0xE5, 0xAA, 0x44)),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("fs: {} Hz | quantum: {}", app.config.sample_rate, app.config.quantum))
                    .font(FontId::monospace(11.0))
                    .color(Color32::from_rgb(0x55, 0x58, 0x78)),
            );
        });
    });
}

fn render_band_list(ui: &mut egui::Ui, app: &mut IemApp) {
    ui.label(
        RichText::new("PEQ Bands")
            .font(FontId::proportional(13.0))
            .color(Color32::from_rgb(0x8A, 0x8D, 0xA8))
            .strong(),
    );
    ui.separator();

    let mut changed = false;
    for (i, band) in app.config.peq.iter_mut().enumerate() {
        let label = format!(
            "{:>6.0} Hz  {:+.1} dB",
            band.freq, band.gain_db
        );
        ui.horizontal(|ui| {
            if ui.checkbox(&mut band.enabled, "").changed() {
                changed = true;
            }
            let selected = app.selected_band == i;
            let btn = egui::SelectableLabel::new(selected, RichText::new(&label).font(FontId::monospace(11.5)));
            if ui.add(btn).clicked() {
                app.selected_band = i;
            }
        });
    }
    if changed {
        app.push_config();
    }
}

fn render_main_panel(ui: &mut egui::Ui, app: &mut IemApp) {
    // ── Frequency response plot ────────────────────────────────────────────
    widgets::freq_response::show(ui, &app.config);

    ui.separator();

    // ── Split: band editor (left) + spatial (right) ────────────────────────
    ui.columns(2, |cols| {
        render_band_editor(&mut cols[0], app);
        render_spatial_panel(&mut cols[1], app);
    });
}

fn render_band_editor(ui: &mut egui::Ui, app: &mut IemApp) {
    ui.label(
        RichText::new("Band Editor")
            .font(FontId::proportional(14.0))
            .color(Color32::from_rgb(0x5E, 0xEA, 0xD4))
            .strong(),
    );
    ui.separator();

    let band_idx = app.selected_band;
    if band_idx >= app.config.peq.len() {
        ui.label("No band selected.");
        return;
    }

    let band = &mut app.config.peq[band_idx];
    let mut changed = false;

    // Filter type selector
    ui.horizontal(|ui| {
        ui.label("Type:");
        let types = [
            FilterType::Peaking, FilterType::LowShelf, FilterType::HighShelf,
            FilterType::LowPass, FilterType::HighPass, FilterType::Notch, FilterType::AllPass,
        ];
        let names = ["Peak", "LowShelf", "HiShelf", "LowPass", "HiPass", "Notch", "AllPass"];
        for (t, n) in types.iter().zip(names.iter()) {
            let selected = band.filter_type == *t;
            if ui.selectable_label(selected, *n).clicked() && !selected {
                band.filter_type = *t;
                changed = true;
            }
        }
    });

    ui.add_space(4.0);

    // Frequency slider (20 – 20000 Hz, log-scaled via drag_value)
    ui.horizontal(|ui| {
        ui.label("Freq (Hz): ");
        let freq_speed = band.freq * 0.005;
        if ui.add(
            egui::DragValue::new(&mut band.freq)
                .speed(freq_speed)
                .range(20.0..=20_000.0)
                .suffix(" Hz"),
        ).changed() { changed = true; }
    });

    // Gain slider (±18 dB)
    ui.horizontal(|ui| {
        ui.label("Gain (dB): ");
        if ui.add(
            egui::Slider::new(&mut band.gain_db, -18.0..=18.0)
                .step_by(0.1)
                .suffix(" dB")
                .fixed_decimals(1),
        ).changed() { changed = true; }
    });

    // Q factor
    ui.horizontal(|ui| {
        ui.label("Q:         ");
        if ui.add(
            egui::Slider::new(&mut band.q, 0.1..=20.0)
                .step_by(0.05)
                .fixed_decimals(2)
                .logarithmic(true),
        ).changed() { changed = true; }
    });

    if changed {
        app.push_config();
    }
}

fn render_spatial_panel(ui: &mut egui::Ui, app: &mut IemApp) {
    use egui::Color32;
    use egui::RichText;
    use egui::FontId;

    ui.label(
        RichText::new("3D Spatial Audio")
            .font(FontId::proportional(14.0))
            .color(Color32::from_rgb(0xC8, 0xA2, 0xFF))
            .strong(),
    );
    ui.separator();

    // ── Mode selector ─────────────────────────────────────────────────────
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mode:").color(Color32::from_rgb(0x8A, 0x8D, 0xA8)));
        for (mode, label) in [
            (SpatialMode::Off,       "Off"),
            (SpatialMode::Crossfeed, "BS2B"),
            (SpatialMode::Hrtf,      "HRTF 3D"),
        ] {
            let selected = app.config.spatial.mode == mode;
            if ui.selectable_label(selected, label).clicked() && !selected {
                app.config.spatial.mode = mode;
                changed = true;
            }
        }
    });

    ui.add_space(6.0);

    match app.config.spatial.mode {
        SpatialMode::Off => {
            ui.label(
                RichText::new("Spatial processing disabled. Pure stereo output.")
                    .color(Color32::from_rgb(0x55, 0x58, 0x78))
                    .font(FontId::proportional(11.0)),
            );
        }

        SpatialMode::Crossfeed => {
            // ── BS2B controls ──────────────────────────────────────────
            let cf = &mut app.config.crossfeed;
            ui.horizontal(|ui| {
                ui.label("Cutoff (Hz):");
                if ui.add(egui::Slider::new(&mut cf.cutoff_hz, 200.0..=2000.0).suffix(" Hz")).changed() {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Feed level: ");
                if ui.add(egui::Slider::new(&mut cf.feed_level, 0.0..=0.9).fixed_decimals(2)).changed() {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("IATD (smpls):");
                if ui.add(egui::DragValue::new(&mut cf.delay_samples).speed(0.5).range(0.0..=100.0)).changed() {
                    changed = true;
                }
            });
            let iatd_ms = cf.delay_samples / app.config.sample_rate as f32 * 1000.0;
            ui.label(
                RichText::new(format!("IATD ≈ {:.3} ms", iatd_ms))
                    .font(FontId::monospace(11.0))
                    .color(Color32::from_rgb(0x55, 0x58, 0x78)),
            );
        }

        SpatialMode::Hrtf => {
            // ── HRTF 3D stage pad ──────────────────────────────────────
            ui.label(
                RichText::new("Drag the L/R dots to position virtual speakers")
                    .color(Color32::from_rgb(0x55, 0x58, 0x78))
                    .font(FontId::proportional(11.0)),
            );
            ui.add_space(4.0);

            let h = &mut app.config.spatial.hrtf;
            if widgets::spatial_pad::show(ui, h) {
                changed = true;
            }
        }
    }

    // ── Output gain ──────────────────────────────────────────────────────
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Output").color(Color32::from_rgb(0x8A, 0x8D, 0xA8)));
    ui.horizontal(|ui| {
        ui.label("Gain (dB):");
        if ui.add(
            egui::Slider::new(&mut app.config.output_gain_db, -20.0..=6.0)
                .suffix(" dB")
                .fixed_decimals(1),
        ).changed() {
            changed = true;
        }
    });

    if changed { app.push_config(); }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("iem_ui=info")
        .init();

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("KZ Castor DSP — iem-ui")
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([700.0, 500.0])
            .with_icon(egui::viewport::IconData::default()),
        ..Default::default()
    };

    eframe::run_native(
        "iem-ui",
        options,
        Box::new(|cc| Ok(Box::new(IemApp::new(cc)))),
    )
}

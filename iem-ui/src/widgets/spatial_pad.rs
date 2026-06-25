//! `widgets/spatial_pad.rs` — Interactive 3D stage view.
//!
//! Draws a circular "head-from-above" view.  The listener is at the centre.
//! Two draggable dots represent the left (cyan) and right (purple) virtual
//! speakers.  Dragging changes their azimuth angle; an elevation slider is
//! shown alongside.
//!
//! ```text
//!            FRONT (0°)
//!                 △
//!         ·  ·  [L]  ·  ·
//!       ·              ·
//!  -90°·    ╭──────╮    ·+90°
//!      ·    │ HEAD │    ·
//!  LEFT·    ╰──────╯    ·RIGHT
//!       ·              ·
//!         ·  ·  [R]  ·
//!                 ▽
//!            BACK (180°)
//! ```

use egui::{Color32, FontId, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

const TEAL:   Color32 = Color32::from_rgb(0x1A, 0xA8, 0x8E);
const PURPLE: Color32 = Color32::from_rgb(0xC8, 0xA2, 0xFF);
const DIM:    Color32 = Color32::from_rgb(0x33, 0x36, 0x50);
const LABEL:  Color32 = Color32::from_rgb(0x8A, 0x8D, 0xA8);

/// Returns `true` if any value changed.
pub fn show(
    ui: &mut Ui,
    cfg: &mut iem_common::HrtfConfig,
) -> bool {
    let mut changed = false;

    // ── Pad ──────────────────────────────────────────────────────────────
    let pad_size = 220.0_f32;
    let (rect, _response) = ui.allocate_exact_size(Vec2::splat(pad_size), Sense::hover());

    let center = rect.center();
    let radius = pad_size * 0.44;
    let painter = ui.painter_at(rect);

    // Background circle
    painter.circle_filled(center, radius, Color32::from_rgb(0x0E, 0x10, 0x18));
    painter.circle_stroke(center, radius, Stroke::new(1.0, DIM));

    // Angle rings (30° increments)
    for r_frac in [0.33_f32, 0.66, 1.0] {
        painter.circle_stroke(center, radius * r_frac, Stroke::new(0.5, Color32::from_rgb(0x22, 0x24, 0x38)));
    }

    // Cardinal lines
    for angle_deg in [0.0_f32, 90.0, 180.0, 270.0] {
        let a = angle_deg.to_radians();
        let outer = center + Vec2::new(a.sin(), -a.cos()) * radius;
        let inner = center + Vec2::new(a.sin(), -a.cos()) * (radius * 0.08);
        painter.line_segment([inner, outer], Stroke::new(0.5, DIM));
    }

    // Cardinal labels
    let labels = [("FRONT", 0.0_f32), ("R", 90.0), ("BACK", 180.0), ("L", 270.0)];
    for (text, deg) in labels {
        let a    = deg.to_radians();
        let pos  = center + Vec2::new(a.sin(), -a.cos()) * (radius + 12.0);
        painter.text(pos, egui::Align2::CENTER_CENTER, text,
            FontId::monospace(9.0), LABEL);
    }

    // Head icon at centre
    painter.circle_filled(center, 14.0, Color32::from_rgb(0x22, 0x25, 0x33));
    painter.circle_stroke(center, 14.0, Stroke::new(1.5, Color32::from_rgb(0x44, 0x48, 0x70)));
    // Nose dot
    painter.circle_filled(center + Vec2::new(0.0, -10.0), 3.0, LABEL);

    // ── Draggable speaker dots ────────────────────────────────────────────
    changed |= speaker_dot(ui, &painter, center, radius, &mut cfg.left_azimuth_deg, TEAL,   "L", rect, pad_size);
    changed |= speaker_dot(ui, &painter, center, radius, &mut cfg.right_azimuth_deg, PURPLE, "R", rect, pad_size);

    // ── Angle readout ─────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("L {:.0}°", cfg.left_azimuth_deg)).color(TEAL).font(FontId::monospace(11.0)));
        ui.add_space(12.0);
        ui.label(RichText::new(format!("R {:.0}°", cfg.right_azimuth_deg)).color(PURPLE).font(FontId::monospace(11.0)));
    });

    // ── Elevation slider ──────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(RichText::new("Elevation:").color(LABEL).font(FontId::monospace(11.0)));
        if ui.add(
            egui::Slider::new(&mut cfg.elevation_deg, -30.0..=60.0)
                .suffix("°")
                .fixed_decimals(0)
        ).changed() { changed = true; }
    });
    
    // ── Crazy 3D Features ─────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(RichText::new("Head Size:").color(LABEL).font(FontId::monospace(11.0)));
        if ui.add(
            egui::Slider::new(&mut cfg.head_scale, 0.5..=3.0)
                .suffix("x")
                .fixed_decimals(2)
        ).on_hover_text("Scales ITD delay. Larger head = more 3D depth.").changed() { changed = true; }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Auto-Spin:").color(LABEL).font(FontId::monospace(11.0)));
        if ui.add(
            egui::Slider::new(&mut cfg.auto_spin_hz, 0.0..=5.0)
                .suffix(" Hz")
                .fixed_decimals(2)
        ).on_hover_text("Speakers orbit around your head.").changed() { changed = true; }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Room Size:").color(LABEL).font(FontId::monospace(11.0)));
        if ui.add(
            egui::Slider::new(&mut cfg.room_reverb, 0.0..=0.99)
                .fixed_decimals(2)
        ).on_hover_text("Adds spatial reflections (reverb).").changed() { changed = true; }
    });

    // ── Wet/dry slider ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(RichText::new("Wet Mix:  ").color(LABEL).font(FontId::monospace(11.0)));
        if ui.add(
            egui::Slider::new(&mut cfg.wet, 0.0..=1.0)
                .fixed_decimals(2)
        ).changed() { changed = true; }
    });

    // ── Quick presets ─────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.small_button("Stereo (±30°)").clicked() {
            cfg.left_azimuth_deg = -30.0; cfg.right_azimuth_deg = 30.0; cfg.auto_spin_hz = 0.0; changed = true;
        }
        if ui.small_button("Wide (±60°)").clicked() {
            cfg.left_azimuth_deg = -60.0; cfg.right_azimuth_deg = 60.0; cfg.auto_spin_hz = 0.0; changed = true;
        }
        if ui.small_button("Surround (±90°)").clicked() {
            cfg.left_azimuth_deg = -90.0; cfg.right_azimuth_deg = 90.0; cfg.auto_spin_hz = 0.0; changed = true;
        }
        if ui.small_button("Theater").clicked() {
            cfg.left_azimuth_deg = -45.0; cfg.right_azimuth_deg = 45.0; cfg.elevation_deg = 15.0; cfg.room_reverb = 0.6; cfg.head_scale = 1.3; changed = true;
        }
    });

    changed
}

/// Draws one draggable speaker dot and handles drag interaction.
/// Returns true if the angle changed.
fn speaker_dot(
    ui:      &mut Ui,
    painter: &Painter,
    center:  Pos2,
    radius:  f32,
    az_deg:  &mut f32,
    color:   Color32,
    label:   &str,
    rect:    Rect,
    _pad_size: f32,
) -> bool {
    let az_rad = az_deg.to_radians();
    let dot_pos = center + Vec2::new(az_rad.sin(), -az_rad.cos()) * (radius * 0.80);

    // Interaction area — slightly larger than the visual dot
    let dot_rect = Rect::from_center_size(dot_pos, Vec2::splat(22.0));
    let response: Response = ui.allocate_rect(dot_rect, Sense::drag());

    // Visual
    let vis_color = if response.hovered() || response.dragged() {
        color
    } else {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 180)
    };
    painter.circle_filled(dot_pos, 8.0, vis_color);
    painter.circle_stroke(dot_pos, 8.0, Stroke::new(1.5, Color32::WHITE));
    painter.text(dot_pos, egui::Align2::CENTER_CENTER, label,
        FontId::monospace(9.0), Color32::BLACK);

    // Connection line from center to dot
    painter.line_segment(
        [center, dot_pos],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 80)),
    );

    // Drag handling: compute new angle from mouse position relative to center
    if response.dragged() {
        if let Some(ptr) = ui.ctx().pointer_interact_pos() {
            if rect.contains(ptr) {
                let delta = ptr - center;
                // atan2: note Y is flipped (screen Y increases downward)
                // atan2(x, -y) gives azimuth where 0=up=front
                let new_az_rad = delta.x.atan2(-delta.y);
                let new_az_deg = new_az_rad.to_degrees();
                // Snap to 5° increments when close
                let snapped = (new_az_deg / 5.0).round() * 5.0;
                *az_deg = snapped.clamp(-180.0, 180.0);
                return true;
            }
        }
    }

    false
}

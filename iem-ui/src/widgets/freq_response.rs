//! `widgets/freq_response.rs` — Frequency response curve widget.
//!
//! Computes and renders the combined magnitude response of all active PEQ
//! bands using `egui_plot`. Calculation is pure Rust scalar math — no GPU.

use egui::{Color32, Stroke};
use egui_plot::{Line, Plot, PlotPoints};
use iem_common::{DspConfig, FilterType};
use std::f32::consts::PI;

/// Number of points in the frequency response curve.
const CURVE_POINTS: usize = 512;
/// Frequency range: 20 Hz – 20 kHz.
const F_MIN: f32 = 20.0;
const F_MAX: f32 = 20_000.0;

/// Compute magnitude response (in dB) of a single biquad at frequency `f`.
fn biquad_magnitude_db(
    freq: f32,
    gain_db: f32,
    q: f32,
    kind: FilterType,
    fs: f32,
    test_f: f32,
) -> f32 {
    // Normalised angular frequency of the test point.
    let w = 2.0 * PI * test_f / fs;
    let w0 = 2.0 * PI * freq / fs;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q);
    let a = 10_f32.powf(gain_db / 40.0);

    let (b0, b1, b2, a0, a1, a2) = match kind {
        FilterType::Peaking => (
            1.0 + alpha * a, -2.0 * cos_w0, 1.0 - alpha * a,
            1.0 + alpha / a, -2.0 * cos_w0, 1.0 - alpha / a,
        ),
        FilterType::LowShelf => {
            let ap1 = a + 1.0; let am1 = a - 1.0; let sq = 2.0 * a.sqrt() * alpha;
            (a*(ap1-am1*cos_w0+sq), 2.0*a*(am1-ap1*cos_w0), a*(ap1-am1*cos_w0-sq),
             ap1+am1*cos_w0+sq, -2.0*(am1+ap1*cos_w0), ap1+am1*cos_w0-sq)
        }
        FilterType::HighShelf => {
            let ap1 = a + 1.0; let am1 = a - 1.0; let sq = 2.0 * a.sqrt() * alpha;
            (a*(ap1+am1*cos_w0+sq), -2.0*a*(am1+ap1*cos_w0), a*(ap1+am1*cos_w0-sq),
             ap1-am1*cos_w0+sq, 2.0*(am1-ap1*cos_w0), ap1-am1*cos_w0-sq)
        }
        FilterType::LowPass => (
            (1.0-cos_w0)/2.0, 1.0-cos_w0, (1.0-cos_w0)/2.0,
            1.0+alpha, -2.0*cos_w0, 1.0-alpha,
        ),
        FilterType::HighPass => (
            (1.0+cos_w0)/2.0, -(1.0+cos_w0), (1.0+cos_w0)/2.0,
            1.0+alpha, -2.0*cos_w0, 1.0-alpha,
        ),
        FilterType::Notch => (
            1.0, -2.0*cos_w0, 1.0,
            1.0+alpha, -2.0*cos_w0, 1.0-alpha,
        ),
        FilterType::AllPass => (
            1.0-alpha, -2.0*cos_w0, 1.0+alpha,
            1.0+alpha, -2.0*cos_w0, 1.0-alpha,
        ),
    };

    // H(e^jw) = (b0 + b1·e^{-jw} + b2·e^{-2jw}) / (a0 + a1·e^{-jw} + a2·e^{-2jw})
    // Magnitude via complex evaluation.
    let cos1 = w.cos();
    let cos2 = (2.0 * w).cos();
    let sin1 = w.sin();
    let sin2 = (2.0 * w).sin();

    let num_re = b0 + b1 * cos1 + b2 * cos2;
    let num_im = -(b1 * sin1 + b2 * sin2);
    let den_re = a0 + a1 * cos1 + a2 * cos2;
    let den_im = -(a1 * sin1 + a2 * sin2);

    let num_mag2 = num_re * num_re + num_im * num_im;
    let den_mag2 = den_re * den_re + den_im * den_im;

    if den_mag2 < 1e-30 {
        return 0.0;
    }

    10.0 * (num_mag2 / den_mag2).log10()
}

/// Compute the combined frequency response curve.
pub fn compute_response(cfg: &DspConfig) -> Vec<[f64; 2]> {
    let fs = cfg.sample_rate as f32;
    let out_gain_db = cfg.output_gain_db;

    (0..CURVE_POINTS)
        .map(|i| {
            // Logarithmically spaced frequency axis.
            let t = i as f32 / (CURVE_POINTS - 1) as f32;
            let f = F_MIN * (F_MAX / F_MIN).powf(t);

            let total_db: f32 = cfg
                .peq
                .iter()
                .filter(|b| b.enabled)
                .map(|b| biquad_magnitude_db(b.freq, b.gain_db, b.q, b.filter_type, fs, f))
                .sum::<f32>()
                + out_gain_db;

            [f as f64, total_db as f64]
        })
        .collect()
}

/// Render the frequency response plot into `ui`.
pub fn show(ui: &mut egui::Ui, cfg: &DspConfig) {
    let points = compute_response(cfg);

    let combined = Line::new(PlotPoints::from(points))
        .color(Color32::from_rgb(0x5E, 0xEA, 0xD4)) // teal
        .stroke(Stroke::new(2.0, Color32::from_rgb(0x5E, 0xEA, 0xD4)))
        .name("Combined");

    // Per-band individual curves (dim)
    let fs = cfg.sample_rate as f32;
    let band_lines: Vec<Line> = cfg
        .peq
        .iter()
        .filter(|b| b.enabled)
        .map(|b| {
            let pts: Vec<[f64; 2]> = (0..CURVE_POINTS)
                .map(|i| {
                    let t = i as f32 / (CURVE_POINTS - 1) as f32;
                    let f = F_MIN * (F_MAX / F_MIN).powf(t);
                    let db = biquad_magnitude_db(b.freq, b.gain_db, b.q, b.filter_type, fs, f);
                    [f as f64, db as f64]
                })
                .collect();
            Line::new(PlotPoints::from(pts))
                .color(Color32::from_rgba_premultiplied(180, 180, 255, 60))
                .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(180, 180, 255, 60)))
        })
        .collect();

    Plot::new("freq_response")
        .height(220.0)
        .x_axis_label("Frequency (Hz)")
        .y_axis_label("Gain (dB)")
        .x_grid_spacer(egui_plot::log_grid_spacer(10))
        .y_axis_min_width(40.0)
        .allow_scroll(false)
        .show(ui, |plot_ui| {
            for bl in band_lines {
                plot_ui.line(bl);
            }
            plot_ui.line(combined);
        });
}

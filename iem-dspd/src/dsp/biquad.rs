//! `dsp/biquad.rs` — Direct-Form II Transposed biquad filter.
//!
//! # Why DF2T?
//! Direct-Form II Transposed minimises the internal signal range, providing
//! better numerical precision at f32 precision — critical for cascaded filters.
//!
//! # No-alloc guarantee
//! `BiquadFilter` is entirely stack-resident (`Copy`). The `process_sample`
//! method is `#[inline(always)]` so the compiler can auto-vectorise the
//! cascade across the 10 bands.

use std::f32::consts::PI;
use iem_common::{FilterType, PeqBand};

/// Biquad coefficients + state (Direct-Form II Transposed).
///
/// State `s1` / `s2` are the two delay elements.
/// All fields are `f32` — keeps the struct at exactly 28 bytes.
#[derive(Debug, Clone, Copy)]
pub struct BiquadFilter {
    // Feed-forward coefficients
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    // Feed-back coefficients (stored as -a1, -a2 for multiply-accumulate)
    pub a1: f32, // = -a1_raw
    pub a2: f32, // = -a2_raw
    // DF2T state
    pub s1: f32,
    pub s2: f32,
}

impl BiquadFilter {
    /// Identity (pass-through) filter.
    pub const fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, s1: 0.0, s2: 0.0 }
    }

    /// Process one sample in-place.
    ///
    /// ```text
    /// DF2T equations:
    ///   y  =  b0·x + s1
    ///   s1 = b1·x - a1·y + s2
    ///   s2 = b2·x - a2·y
    /// ```
    #[inline(always)]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Reset internal state without touching coefficients.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Compute new coefficients from a `PeqBand` definition.
    pub fn from_band(band: &PeqBand, fs: f32) -> Self {
        if !band.enabled {
            return Self::identity();
        }
        compute_biquad(band.filter_type, band.freq, band.gain_db, band.q, fs)
    }
}

/// Biquad coefficient computation for all supported filter types.
///
/// Reference: Audio EQ Cookbook, Robert Bristow-Johnson.
fn compute_biquad(kind: FilterType, freq: f32, gain_db: f32, q: f32, fs: f32) -> BiquadFilter {
    let w0 = 2.0 * PI * freq / fs;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q);
    let a = 10_f32.powf(gain_db / 40.0); // sqrt(10^(dB/20))

    let (b0, b1, b2, a0, a1_raw, a2_raw) = match kind {
        FilterType::Peaking => {
            let b0 = 1.0 + alpha * a;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0 - alpha * a;
            let a0 = 1.0 + alpha / a;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha / a;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::LowShelf => {
            let ap1 = a + 1.0;
            let am1 = a - 1.0;
            let sq = 2.0 * a.sqrt() * alpha;
            let b0 = a * (ap1 - am1 * cos_w0 + sq);
            let b1 = 2.0 * a * (am1 - ap1 * cos_w0);
            let b2 = a * (ap1 - am1 * cos_w0 - sq);
            let a0 = ap1 + am1 * cos_w0 + sq;
            let a1 = -2.0 * (am1 + ap1 * cos_w0);
            let a2 = ap1 + am1 * cos_w0 - sq;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::HighShelf => {
            let ap1 = a + 1.0;
            let am1 = a - 1.0;
            let sq = 2.0 * a.sqrt() * alpha;
            let b0 = a * (ap1 + am1 * cos_w0 + sq);
            let b1 = -2.0 * a * (am1 + ap1 * cos_w0);
            let b2 = a * (ap1 + am1 * cos_w0 - sq);
            let a0 = ap1 - am1 * cos_w0 + sq;
            let a1 = 2.0 * (am1 - ap1 * cos_w0);
            let a2 = ap1 - am1 * cos_w0 - sq;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::LowPass => {
            let b0 = (1.0 - cos_w0) / 2.0;
            let b1 = 1.0 - cos_w0;
            let b2 = (1.0 - cos_w0) / 2.0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::HighPass => {
            let b0 = (1.0 + cos_w0) / 2.0;
            let b1 = -(1.0 + cos_w0);
            let b2 = (1.0 + cos_w0) / 2.0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::Notch => {
            let b0 = 1.0;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::AllPass => {
            let b0 = 1.0 - alpha;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0 + alpha;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
    };

    // Normalise by a0; store negated a1/a2 for the DF2T accumulation.
    BiquadFilter {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1_raw / a0,   // NOTE: sign convention: a1_stored = a1_raw/a0 (NOT negated)
        a2: a2_raw / a0,   // process_sample uses:  y = b0·x + s1
                           //   s1 = b1·x - a1·y + s2     ← subtraction of stored value
        s1: 0.0,
        s2: 0.0,
    }
}

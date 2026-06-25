//! `dsp/crossfeed.rs` — BS2B (Bauer stereophonic-to-binaural) crossfeed.
//!
//! # Algorithm
//!
//! BS2B simulates the acoustic crosstalk that occurs in loudspeaker listening:
//! each ear hears both channels, with the opposite channel delayed and
//! low-pass filtered.
//!
//! ```text
//! L_out = L + feed * LPF(R_delayed)
//! R_out = R + feed * LPF(L_delayed)
//! ```
//!
//! The low-pass is a first-order IIR shelf that models the HRTF pinna
//! shadow. The inter-aural delay is implemented as a short circular buffer
//! (`DelayLine`).
//!
//! # Real-time safety
//!
//! `DelayLine` is a fixed-size stack-allocated ring buffer. No heap allocation
//! occurs after construction. `CrossfeedProcessor::process_stereo_block` is
//! `#[inline]` and allocation-free.

use std::f32::consts::PI;
use iem_common::CrossfeedConfig;

/// Maximum supported delay in samples (≤ 2 ms @ 192 kHz = 384).
const MAX_DELAY: usize = 384;

/// First-order one-pole IIR low-pass for the crossfeed shelf.
///
/// `y[n] = (1-c)·x[n] + c·y[n-1]`  where  `c = exp(-2π·fc/fs)`.
#[derive(Clone, Copy, Debug)]
struct OnePoleLP {
    c: f32,   // pole coefficient
    z: f32,   // single delay element
}

impl OnePoleLP {
    fn new(cutoff_hz: f32, fs: f32) -> Self {
        let c = (-2.0 * PI * cutoff_hz / fs).exp();
        Self { c, z: 0.0 }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y = (1.0 - self.c) * x + self.c * self.z;
        self.z = y;
        y
    }

    fn reset(&mut self) { self.z = 0.0; }
}

/// Fractional delay line implemented as a power-of-two circular buffer.
///
/// Integer-sample delay only; fractional rounding to nearest integer.
/// For sub-sample accuracy, replace `read()` with linear interpolation.
#[derive(Clone, Debug)]
struct DelayLine {
    buf: [f32; MAX_DELAY],
    write: usize,
    delay: usize,
}

impl DelayLine {
    fn new(delay_samples: f32) -> Self {
        let d = (delay_samples.round() as usize).clamp(0, MAX_DELAY - 1);
        Self { buf: [0.0; MAX_DELAY], write: 0, delay: d }
    }

    fn set_delay(&mut self, delay_samples: f32) {
        self.delay = (delay_samples.round() as usize).clamp(0, MAX_DELAY - 1);
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        self.buf[self.write] = x;
        // Read index wraps
        let read = if self.write >= self.delay {
            self.write - self.delay
        } else {
            MAX_DELAY - self.delay + self.write
        };
        self.write = (self.write + 1) % MAX_DELAY;
        self.buf[read]
    }

    fn reset(&mut self) {
        self.buf = [0.0; MAX_DELAY];
        self.write = 0;
    }
}

/// Full BS2B crossfeed processor for a stereo stream.
#[derive(Clone, Debug)]
pub struct CrossfeedProcessor {
    enabled: bool,
    feed: f32,
    lp_l: OnePoleLP,   // LPF applied to L before adding to R
    lp_r: OnePoleLP,   // LPF applied to R before adding to L
    dl_l: DelayLine,   // Delay on L cross path
    dl_r: DelayLine,   // Delay on R cross path
}

impl CrossfeedProcessor {
    pub fn new(cfg: &CrossfeedConfig, fs: f32) -> Self {
        Self {
            enabled: cfg.enabled,
            feed: cfg.feed_level,
            lp_l: OnePoleLP::new(cfg.cutoff_hz, fs),
            lp_r: OnePoleLP::new(cfg.cutoff_hz, fs),
            dl_l: DelayLine::new(cfg.delay_samples),
            dl_r: DelayLine::new(cfg.delay_samples),
        }
    }

    /// Update coefficients from new config without resetting state.
    pub fn update(&mut self, cfg: &CrossfeedConfig, fs: f32) {
        self.enabled = cfg.enabled;
        self.feed    = cfg.feed_level;
        let c = (-2.0 * PI * cfg.cutoff_hz / fs).exp();
        self.lp_l.c = c;
        self.lp_r.c = c;
        self.dl_l.set_delay(cfg.delay_samples);
        self.dl_r.set_delay(cfg.delay_samples);
    }

    pub fn reset(&mut self) {
        self.lp_l.reset();
        self.lp_r.reset();
        self.dl_l.reset();
        self.dl_r.reset();
    }

    /// Process an interleaved stereo buffer in-place: `[L0, R0, L1, R1, ...]`.
    ///
    /// **Zero-alloc.** Called from the RT audio callback.
    #[inline]
    pub fn process_stereo_block(&mut self, buf: &mut [f32]) {
        if !self.enabled {
            return;
        }
        debug_assert_eq!(buf.len() % 2, 0, "interleaved buffer must be even-length");

        let feed = self.feed;
        let mut i = 0;
        while i < buf.len() {
            let l = buf[i];
            let r = buf[i + 1];

            // Cross path: L → delay → LP → scale → add to R
            let l_cross = self.lp_l.process(self.dl_l.process(l));
            // Cross path: R → delay → LP → scale → add to L
            let r_cross = self.lp_r.process(self.dl_r.process(r));

            buf[i]     = l + feed * r_cross;
            buf[i + 1] = r + feed * l_cross;

            i += 2;
        }
    }
}

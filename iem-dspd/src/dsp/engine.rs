//! `dsp/engine.rs` — RT-safe DSP engine.
//!
//! # Pipeline (per process quantum)
//! ```text
//! Input (interleaved stereo f32)
//!   ↓
//!   [1] 10-band DF2T PEQ cascade (L + R independently)
//!   ↓
//!   [2] Spatial processing — one of:
//!       • Off       → straight stereo passthrough
//!       • Crossfeed → BS2B loudspeaker crossfeed simulation
//!       • Hrtf      → Woodworth spherical-head binaural model
//!   ↓
//!   [3] Output gain (linear scalar)
//!   ↓
//! Output (interleaved stereo f32, same buffer)
//! ```
//!
//! **No heap allocations. No mutex acquisition. No system calls.**

use std::sync::Arc;

use arc_swap::ArcSwap;
use iem_common::{DspConfig, SpatialMode};

use super::biquad::BiquadFilter;
use super::crossfeed::CrossfeedProcessor;
use super::hrtf::HrtfProcessor;

const MAX_BANDS: usize = 10;

pub struct DspEngine {
    sample_rate: f32,

    peq_l: [BiquadFilter; MAX_BANDS],
    peq_r: [BiquadFilter; MAX_BANDS],
    active_bands: usize,

    output_gain: f32,

    /// BS2B crossfeed (active when mode == Crossfeed).
    crossfeed: CrossfeedProcessor,
    /// HRTF binaural model (active when mode == Hrtf).
    hrtf: HrtfProcessor,

    /// Currently active spatial mode — cached to avoid re-reading config.
    spatial_mode: SpatialMode,

    config: Arc<ArcSwap<DspConfig>>,
    last_config_ptr: *const DspConfig,
}

unsafe impl Send for DspEngine {}

impl DspEngine {
    pub fn new(cfg: Arc<ArcSwap<DspConfig>>) -> Self {
        let owned = cfg.load_full();
        let fs = owned.sample_rate as f32;

        let mut engine = Self {
            sample_rate: fs,
            peq_l: [BiquadFilter::identity(); MAX_BANDS],
            peq_r: [BiquadFilter::identity(); MAX_BANDS],
            active_bands: 0,
            output_gain: 1.0,
            crossfeed:    CrossfeedProcessor::new(&owned.crossfeed, fs),
            hrtf:         HrtfProcessor::new(&owned.spatial, fs),
            spatial_mode: owned.spatial.mode,
            last_config_ptr: Arc::as_ptr(&owned),
            config: cfg,
        };
        engine.apply_config(&owned);
        engine
    }

    fn apply_config(&mut self, cfg: &DspConfig) {
        let fs = cfg.sample_rate as f32;
        self.sample_rate = fs;

        let n = cfg.peq.len().min(MAX_BANDS);
        self.active_bands = n;

        for i in 0..n {
            self.peq_l[i] = BiquadFilter::from_band(&cfg.peq[i], fs);
            self.peq_r[i] = BiquadFilter::from_band(&cfg.peq[i], fs);
        }
        for i in n..MAX_BANDS {
            self.peq_l[i] = BiquadFilter::identity();
            self.peq_r[i] = BiquadFilter::identity();
        }

        self.output_gain  = 10_f32.powf(cfg.output_gain_db / 20.0);
        self.crossfeed.update(&cfg.crossfeed, fs);
        self.hrtf.update(&cfg.spatial, fs);
        self.spatial_mode = cfg.spatial.mode;
    }

    #[inline]
    pub fn process_block(&mut self, buf: &mut [f32]) {
        // ── 0. Lock-free config update check ──────────────────────────────────
        {
            let owned = self.config.load_full();
            let new_ptr = Arc::as_ptr(&owned);
            if new_ptr != self.last_config_ptr {
                self.apply_config(&owned);
                self.last_config_ptr = new_ptr;
            }
        }

        let n    = self.active_bands;
        let gain = self.output_gain;

        // ── 1. 10-band PEQ (both channels, per-frame) ─────────────────────────
        let mut i = 0;
        while i < buf.len() {
            let mut l = buf[i];
            let mut r = buf[i + 1];

            for k in 0..n {
                // SAFETY: k < MAX_BANDS guaranteed by apply_config.
                l = unsafe { self.peq_l.get_unchecked_mut(k).process_sample(l) };
                r = unsafe { self.peq_r.get_unchecked_mut(k).process_sample(r) };
            }

            buf[i]     = l;
            buf[i + 1] = r;
            i += 2;
        }

        // ── 2. Spatial processing ─────────────────────────────────────────────
        match self.spatial_mode {
            SpatialMode::Off => { /* passthrough */ }
            SpatialMode::Crossfeed => {
                self.crossfeed.process_stereo_block(buf);
            }
            SpatialMode::Hrtf => {
                self.hrtf.process_stereo_block(buf);
            }
        }

        // ── 3. Output gain ────────────────────────────────────────────────────
        if (gain - 1.0).abs() > 1e-6 {
            for s in buf.iter_mut() {
                *s *= gain;
            }
        }
    }
}

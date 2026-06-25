//! `dsp/hrtf.rs` — Woodworth spherical-head HRTF binaural model.
//!
//! # Model
//!
//! For each virtual speaker at azimuth θ and elevation φ:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  Source signal x[n]                                                  │
//! │       │                                                              │
//! │       ├──── NEAR EAR path ─────────────────────────────────────────│
//! │       │        ILD boost (high-shelf +G dB above f_ild)             │
//! │       │        Pinna notch (f_notch = 8-10 kHz, -8 dB)             │
//! │       │        → near_out                                           │
//! │       │                                                              │
//! │       └──── FAR EAR path  ─────────────────────────────────────────│
//! │                ITD delay (τ samples, Woodworth formula)             │
//! │                ILD cut   (high-shelf -G dB above f_ild)             │
//! │                Pinna notch (attenuated)                             │
//! │                → far_out                                            │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Woodworth ITD formula
//! ```text
//! τ(θ) = (a/c) × (θ_rad + sin(θ_rad))     for |θ| ≤ 90°
//! τ(θ) = (a/c) × (π - θ_rad + sin(θ_rad)) for |θ| > 90°
//!
//! a = 0.0875 m (average adult head radius)
//! c = 343.0 m/s (speed of sound)
//! τ_max ≈ 0.69 ms ≈ 33 samples @ 48 kHz
//! ```
//!
//! # Real-time safety
//!
//! All state is stack-resident.  `process_stereo_block` makes no allocations.

use std::f32::consts::PI;
use iem_common::{HrtfConfig, SpatialConfig, SpatialMode};
use super::biquad::BiquadFilter;

/// Maximum ITD in samples — covers any head size at any sample rate.
const MAX_ITD: usize = 128; // Increased for larger head scale


// ---------------------------------------------------------------------------
// Delay line (integer samples, ring buffer)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Delay {
    buf: [f32; MAX_ITD],
    write: usize,
    len: usize,
}

impl Delay {
    fn new(samples: usize) -> Self {
        Self { buf: [0.0; MAX_ITD], write: 0, len: samples.clamp(0, MAX_ITD - 1) }
    }

    fn set(&mut self, samples: usize) {
        self.len = samples.clamp(0, MAX_ITD - 1);
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        self.buf[self.write] = x;
        let read = if self.write >= self.len {
            self.write - self.len
        } else {
            MAX_ITD - self.len + self.write
        };
        self.write = (self.write + 1) % MAX_ITD;
        self.buf[read]
    }
}

// ---------------------------------------------------------------------------
// Virtual speaker — one azimuth/elevation position
// ---------------------------------------------------------------------------

/// Models one virtual point source in 3D space.
///
/// Given a monophonic signal `x`, it returns `(left_ear, right_ear)` samples
/// encoding the directional cues for that source position.
#[derive(Clone, Debug)]
pub struct VirtualSpeaker {
    /// Azimuth in degrees: 0=front, -90=left, +90=right, ±180=back
    azimuth_deg: f32,
    /// Elevation in degrees: 0=horizontal, +90=above
    elevation_deg: f32,

    // ── Far-ear ITD delay ─────────────────────────────────────────────────
    itd_delay: Delay,

    // ── ILD: high-frequency level difference (head shadow) ────────────────
    /// High-shelf boost on the near ear
    ild_near: BiquadFilter,
    /// High-shelf cut on the far ear
    ild_far: BiquadFilter,

    // ── Pinna notch: spectral cue for elevation & front/back ──────────────
    /// Notch on near ear (stronger)
    pinna_near: BiquadFilter,
    /// Notch on far ear (attenuated)
    pinna_far: BiquadFilter,

    /// true when source is on the left (left ear = near)
    source_left: bool,
    /// Saved head scale
    head_scale: f32,
}


impl VirtualSpeaker {
    pub fn new(azimuth_deg: f32, elevation_deg: f32, head_scale: f32, fs: f32) -> Self {
        let mut s = Self {
            azimuth_deg,
            elevation_deg,
            itd_delay: Delay::new(0),
            ild_near:  BiquadFilter::identity(),
            ild_far:   BiquadFilter::identity(),
            pinna_near: BiquadFilter::identity(),
            pinna_far:  BiquadFilter::identity(),
            source_left: false,
            head_scale,
        };
        s.recompute(azimuth_deg, elevation_deg, head_scale, fs);
        s
    }

    pub fn update(&mut self, azimuth_deg: f32, elevation_deg: f32, head_scale: f32, fs: f32) {
        if (self.azimuth_deg - azimuth_deg).abs() > 0.01
            || (self.elevation_deg - elevation_deg).abs() > 0.01
            || (self.head_scale - head_scale).abs() > 0.01
        {
            self.recompute(azimuth_deg, elevation_deg, head_scale, fs);
            self.azimuth_deg = azimuth_deg;
            self.elevation_deg = elevation_deg;
            self.head_scale = head_scale;
        }
    }

    /// Recompute ITD, ILD, and pinna coefficients from angles.
    fn recompute(&mut self, az_deg: f32, el_deg: f32, head_scale: f32, fs: f32) {
        let az_rad = az_deg.to_radians();
        let el_rad = el_deg.to_radians();

        // ── ITD (Woodworth formula) ────────────────────────────────────────
        let a = 0.0875 * head_scale.max(0.1); // Scaled head radius
        const C: f32 = 343.0;  // speed of sound m/s

        let az_abs = az_rad.abs().clamp(0.0, PI);
        let tau_sec = if az_abs <= PI / 2.0 {
            (a / C) * (az_abs + az_abs.sin())
        } else {
            (a / C) * (PI - az_abs + az_abs.sin().abs())
        };
        let itd_samples = (tau_sec * fs).round() as usize;
        self.itd_delay.set(itd_samples.min(MAX_ITD - 1));

        // Source direction: negative azimuth = left side
        self.source_left = az_deg <= 0.0;

        // ── ILD: high-shelf at 1200 Hz ─────────────────────────────────────
        // Gain scales with sin(azimuth) — maxes out at ±90°.
        // Near ear: +ild_db, far ear: -ild_db
        let ild_db = 10.0 * az_abs.sin();   // 0–10 dB range
        let f_ild  = 1_200.0_f32;
        let q_ild  = 0.71_f32;

        self.ild_near = high_shelf(f_ild,  ild_db, q_ild, fs);
        self.ild_far  = high_shelf(f_ild, -ild_db, q_ild, fs);

        // ── Pinna notch: elevation cue ─────────────────────────────────────
        // f_notch shifts with elevation: 8 kHz at 0°, up to 11 kHz at +90°.
        let f_notch = 8_000.0 + 3_000.0 * el_rad.sin();
        let f_notch = f_notch.clamp(4_000.0, 14_000.0);

        self.pinna_near = notch(f_notch, -9.0, 3.5, fs);
        self.pinna_far  = notch(f_notch, -4.0, 3.5, fs); // attenuated for far ear
    }

    /// Process one sample.
    ///
    /// Returns `(left_ear_sample, right_ear_sample)`.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let x_delayed = self.itd_delay.process(x);

        // Distinguish near/far based on azimuth side
        let (near_src, far_src) = if self.source_left {
            // Source is on the LEFT → left ear is near (no delay), right ear is far (delayed)
            (x, x_delayed)
        } else {
            // Source is on the RIGHT → right ear is near, left ear is far
            (x, x_delayed)
        };

        // Apply ILD
        let near_ild = self.ild_near.process_sample(near_src);
        let far_ild  = self.ild_far.process_sample(far_src);

        // Apply pinna notch
        let near_out = self.pinna_near.process_sample(near_ild);
        let far_out  = self.pinna_far.process_sample(far_ild);

        if self.source_left {
            // Left = near, Right = far
            (near_out, far_out)
        } else {
            // Right = near, Left = far
            (far_out, near_out)
        }
    }
}

// ---------------------------------------------------------------------------
// Room Reverb (Freeverb-lite) — zero alloc in hot path
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ReverbDelay {
    buf: Box<[f32]>,
    write: usize,
}
impl ReverbDelay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len].into_boxed_slice(), write: 0 }
    }
    #[inline(always)]
    fn process_comb(&mut self, x: f32, feedback: f32) -> f32 {
        let out = self.buf[self.write];
        self.buf[self.write] = x + out * feedback;
        self.write += 1;
        if self.write >= self.buf.len() { self.write = 0; }
        out
    }
    #[inline(always)]
    fn process_allpass(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.write];
        let out = -x + delayed;
        self.buf[self.write] = x + delayed * 0.5;
        self.write += 1;
        if self.write >= self.buf.len() { self.write = 0; }
        out
    }
}

#[derive(Clone, Debug)]
struct ChannelReverb {
    c1: ReverbDelay, c2: ReverbDelay, c3: ReverbDelay, c4: ReverbDelay,
    a1: ReverbDelay, a2: ReverbDelay,
}
impl ChannelReverb {
    fn new(offset: usize) -> Self {
        Self {
            c1: ReverbDelay::new(1557 + offset),
            c2: ReverbDelay::new(1617 + offset),
            c3: ReverbDelay::new(1491 + offset),
            c4: ReverbDelay::new(1422 + offset),
            a1: ReverbDelay::new(225 + offset),
            a2: ReverbDelay::new(341 + offset),
        }
    }
    #[inline(always)]
    fn process(&mut self, x: f32, room_size: f32) -> f32 {
        let combs = self.c1.process_comb(x, room_size)
                  + self.c2.process_comb(x, room_size - 0.01)
                  + self.c3.process_comb(x, room_size - 0.02)
                  + self.c4.process_comb(x, room_size - 0.03);
        let out = self.a1.process_allpass(combs * 0.25);
        self.a2.process_allpass(out)
    }
}

// ---------------------------------------------------------------------------
// HRTF Processor — stereo-to-binaural upmixer
// ---------------------------------------------------------------------------

/// Full stereo-to-binaural processor.
///
/// Treats the stereo input as two virtual speakers at configurable positions.
/// Applies the Woodworth HRTF model to each channel independently, then mixes
/// the left-ear and right-ear contributions.
#[derive(Clone, Debug)]
pub struct HrtfProcessor {
    enabled: bool,
    wet: f32,
    left_speaker:  VirtualSpeaker,
    right_speaker: VirtualSpeaker,
    
    // Extracted target parameters for LFO
    target_l_az: f32,
    target_r_az: f32,
    target_el: f32,
    head_scale: f32,
    head_yaw_deg: f32,

    // LFO state
    spin_phase: f32,
    spin_hz: f32,

    // Reverb state
    room_reverb: f32,
    reverb_l: ChannelReverb,
    reverb_r: ChannelReverb,
    
    fs: f32,
}

impl HrtfProcessor {
    pub fn new(cfg: &SpatialConfig, fs: f32) -> Self {
        let h = &cfg.hrtf;
        Self {
            enabled: cfg.mode == SpatialMode::Hrtf,
            wet:     h.wet,
            left_speaker:  VirtualSpeaker::new(h.left_azimuth_deg,  h.elevation_deg, h.head_scale, fs),
            right_speaker: VirtualSpeaker::new(h.right_azimuth_deg, h.elevation_deg, h.head_scale, fs),
            target_l_az: h.left_azimuth_deg,
            target_r_az: h.right_azimuth_deg,
            target_el: h.elevation_deg,
            head_scale: h.head_scale,
            head_yaw_deg: h.head_yaw_deg,
            spin_phase: 0.0,
            spin_hz: h.auto_spin_hz,
            room_reverb: h.room_reverb,
            reverb_l: ChannelReverb::new(0),
            reverb_r: ChannelReverb::new(23), // Slight offset for stereo decorrelation
            fs,
        }
    }

    pub fn update(&mut self, cfg: &SpatialConfig, fs: f32) {
        let h = &cfg.hrtf;
        self.enabled = cfg.mode == SpatialMode::Hrtf;
        self.wet     = h.wet;
        self.target_l_az = h.left_azimuth_deg;
        self.target_r_az = h.right_azimuth_deg;
        self.target_el   = h.elevation_deg;
        self.head_scale  = h.head_scale;
        self.head_yaw_deg = h.head_yaw_deg;
        self.spin_hz     = h.auto_spin_hz;
        self.room_reverb = h.room_reverb;
        
        // We defer speaker update to process loop to incorporate LFO smoothly
        self.fs = fs;
    }

    /// Process an interleaved stereo buffer in-place.
    ///
    /// # Signal chain per frame:
    /// ```text
    /// L_in  → left_speaker  HRTF  → (LS_l, LS_r)
    /// R_in  → right_speaker HRTF  → (RS_l, RS_r)
    /// L_out = LS_l + RS_l          (binaural left  ear)
    /// R_out = LS_r + RS_r          (binaural right ear)
    /// output = wet*binaural + (1-wet)*dry
    /// ```
    #[inline]
    pub fn process_stereo_block(&mut self, buf: &mut [f32]) {
        if !self.enabled {
            return;
        }

        // Apply LFO auto-spin if active
        let mut l_az = self.target_l_az;
        let mut r_az = self.target_r_az;
        if self.spin_hz > 0.01 {
            let spin_deg = self.spin_phase * 360.0;
            l_az += spin_deg;
            r_az += spin_deg;
            
            // Advance phase per block (approx 5.3ms for 256 samples @ 48kHz)
            let block_time = (buf.len() / 2) as f32 / self.fs;
            self.spin_phase += self.spin_hz * block_time;
            if self.spin_phase > 1.0 { self.spin_phase -= 1.0; }
        } else {
            self.spin_phase = 0.0; // Reset
        }
        
        // Counter-rotate the stage opposite to the user's head yaw
        let yaw = self.head_yaw_deg;
        l_az -= yaw;
        r_az -= yaw;
        
        // Keep in -180..180
        l_az = (l_az + 180.0).rem_euclid(360.0) - 180.0;
        r_az = (r_az + 180.0).rem_euclid(360.0) - 180.0;
        
        self.left_speaker.update(l_az, self.target_el, self.head_scale, self.fs);
        self.right_speaker.update(r_az, self.target_el, self.head_scale, self.fs);

        let wet = self.wet;
        let dry = 1.0 - wet;
        
        let do_reverb = self.room_reverb > 0.01;
        let r_size = self.room_reverb * 0.95; // Limit max feedback

        let mut i = 0;
        while i < buf.len() {
            let l_in = buf[i];
            let r_in = buf[i + 1];

            // Each virtual speaker returns (left_ear_contribution, right_ear_contribution)
            let (ls_l, ls_r) = self.left_speaker.process(l_in);
            let (rs_l, rs_r) = self.right_speaker.process(r_in);

            let mut l_bin = ls_l + rs_l;
            let mut r_bin = ls_r + rs_r;

            // Apply Reverb
            if do_reverb {
                let l_rev = self.reverb_l.process(l_bin, r_size);
                let r_rev = self.reverb_r.process(r_bin, r_size);
                // Mix reverb in with some dry signal for clarity
                l_bin = l_bin * 0.7 + l_rev * 0.3;
                r_bin = r_bin * 0.7 + r_rev * 0.3;
            }

            buf[i]     = wet * l_bin + dry * l_in;
            buf[i + 1] = wet * r_bin + dry * r_in;

            i += 2;
        }
    }
}

// ---------------------------------------------------------------------------
// Biquad coefficient helpers
// ---------------------------------------------------------------------------

/// High-frequency shelf biquad.
fn high_shelf(freq: f32, gain_db: f32, q: f32, fs: f32) -> BiquadFilter {
    use iem_common::{FilterType, PeqBand};
    BiquadFilter::from_band(&PeqBand {
        freq,
        gain_db,
        q,
        filter_type: FilterType::HighShelf,
        enabled: true,
    }, fs)
}

/// Notch (band-reject) biquad.
fn notch(freq: f32, gain_db: f32, q: f32, fs: f32) -> BiquadFilter {
    // Notch with gain implemented as a Peaking filter with negative gain.
    use iem_common::{FilterType, PeqBand};
    BiquadFilter::from_band(&PeqBand {
        freq,
        gain_db,
        q,
        filter_type: FilterType::Peaking,
        enabled: true,
    }, fs)
}

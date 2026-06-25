//! `iem-common` — Shared types and IPC protocol for iem-dspd / iem-ui.
//!
//! Everything in this crate must be `#[no_std]`-friendly in terms of allocations
//! so the daemon's RT thread can safely read config snapshots through an
//! `Arc<ArcSwap<DspConfig>>` without ever blocking.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TOML configuration schema
// ---------------------------------------------------------------------------

/// Top-level config, loaded from `~/.config/iem-dspd/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DspConfig {
    /// Sample rate the daemon negotiates with PipeWire.
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    /// Block / quantum size (frames). Keep a power-of-two for SIMD alignment.
    #[serde(default = "default_quantum")]
    pub quantum: u32,

    /// Exactly 10 parametric EQ bands (L + R processed identically).
    pub peq: Vec<PeqBand>,

    /// BS2B crossfeed settings.
    pub crossfeed: CrossfeedConfig,

    /// HRTF binaural spatial settings.
    #[serde(default)]
    pub spatial: SpatialConfig,

    /// Output gain in dB applied after all DSP (avoid clipping).
    #[serde(default)]
    pub output_gain_db: f32,
}

fn default_sample_rate() -> u32 { 48_000 }
fn default_quantum()      -> u32 { 256 }

/// Biquad filter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
    AllPass,
}

/// One parametric EQ band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeqBand {
    /// Centre / corner frequency in Hz.
    pub freq: f32,
    /// Gain in dB (ignored for LP/HP/Notch/AP).
    pub gain_db: f32,
    /// Q factor.
    pub q: f32,
    /// Filter type.
    #[serde(rename = "type")]
    pub filter_type: FilterType,
    /// Enable / disable without removing from config.
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool { true }

/// BS2B (Bauer stereophonic-to-binaural) crossfeed parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossfeedConfig {
    /// Crossfeed enabled toggle.
    #[serde(default)]
    pub enabled: bool,
    /// Low-pass cutoff frequency for the crossfeed shelf (Hz). Typical: 700 Hz.
    pub cutoff_hz: f32,
    /// Feed level: gain of the cross-channel signal [0.0 – 1.0]. Typical: 0.45.
    pub feed_level: f32,
    /// Inter-aural time delay in samples (fractional). Typical: ~0.3 ms @ 48 kHz ≈ 14.4.
    pub delay_samples: f32,
}

impl Default for CrossfeedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cutoff_hz: 700.0,
            feed_level: 0.45,
            delay_samples: 14.0,
        }
    }
}

// ---------------------------------------------------------------------------
// HRTF / Spatial audio config
// ---------------------------------------------------------------------------

/// Which spatial processing mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpatialMode {
    /// No spatial processing (straight stereo after PEQ).
    #[default]
    Off,
    /// BS2B loudspeaker crossfeed simulation.
    Crossfeed,
    /// Full HRTF binaural rendering (Woodworth spherical-head model).
    Hrtf,
}

/// Parameters for HRTF-based binaural spatialization.
///
/// The stereo signal is treated as two virtual speakers at configurable
/// azimuth/elevation angles.  The Woodworth spherical-head model computes
/// ITD (delay) and ILD (shelf gain); a pinna notch adds elevation cues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HrtfConfig {
    /// Azimuth of the left virtual speaker in degrees.
    /// 0° = front, -90° = hard left, 180° = directly behind.
    #[serde(default = "default_l_az")]
    pub left_azimuth_deg: f32,

    /// Azimuth of the right virtual speaker in degrees.
    #[serde(default = "default_r_az")]
    pub right_azimuth_deg: f32,

    /// Elevation of both virtual speakers in degrees.
    /// 0° = horizontal, +90° = directly above.
    #[serde(default)]
    pub elevation_deg: f32,

    /// Wet/dry mix [0.0 – 1.0]. 1.0 = full HRTF, 0.0 = bypass.
    #[serde(default = "default_wet")]
    pub wet: f32,

    /// Scale of the head (1.0 = average adult). Larger = more ITD depth.
    #[serde(default = "default_head_scale")]
    pub head_scale: f32,

    /// Auto-rotation speed in Hz (0.0 = stationary).
    #[serde(default)]
    pub auto_spin_hz: f32,

    /// Room reflection level (0.0 = anechoic, 1.0 = concert hall).
    #[serde(default)]
    pub room_reverb: f32,

    /// Live head tracking yaw (degrees). Positive = looking right.
    #[serde(default)]
    pub head_yaw_deg: f32,
}

fn default_l_az()  -> f32 { -30.0 }
fn default_r_az()  -> f32 {  30.0 }
fn default_wet()   -> f32 {   1.0 }
fn default_head_scale() -> f32 { 1.0 }

impl Default for HrtfConfig {
    fn default() -> Self {
        Self {
            left_azimuth_deg:  -30.0,
            right_azimuth_deg:  30.0,
            elevation_deg:       0.0,
            wet:                 1.0,
            head_scale:          1.0,
            auto_spin_hz:        0.0,
            room_reverb:         0.0,
            head_yaw_deg:        0.0,
        }
    }
}

/// Combined spatial config sent in the IPC payload.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpatialConfig {
    /// Active spatial mode.
    pub mode: SpatialMode,
    /// HRTF parameters (used when mode == Hrtf).
    #[serde(default)]
    pub hrtf: HrtfConfig,
}

// ---------------------------------------------------------------------------
// IPC protocol — simple length-prefixed JSON over Unix Domain Socket
// ---------------------------------------------------------------------------

/// Commands sent from UI → Daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    /// Replace the entire running config atomically.
    SetConfig { config: DspConfig },
    /// Toggle a single PEQ band by index.
    SetBandEnabled { band: usize, enabled: bool },
    /// Set a single PEQ band gain.
    SetBandGain { band: usize, gain_db: f32 },
    /// Fast-path realtime head tracking update (does not write to disk).
    SetHeadYaw { yaw_deg: f32 },
    /// Reload config from disk.
    ReloadConfig,
    /// Ask daemon to send back its current config.
    GetConfig,
    /// Graceful shutdown.
    Shutdown,
}

/// Responses sent from Daemon → UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "resp", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok,
    Config { config: DspConfig },
    Error { message: String },
}

// ---------------------------------------------------------------------------
// IPC framing helpers (sync; used by both ends)
// ---------------------------------------------------------------------------

/// Write one IPC frame: 4-byte little-endian length prefix + JSON payload.
pub fn encode_frame(msg: &impl Serialize) -> Vec<u8> {
    let json = serde_json::to_vec(msg).expect("IPC serialization failed");
    let mut buf = Vec::with_capacity(4 + json.len());
    let len = json.len() as u32;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);
    buf
}

/// Parse the length prefix from the first 4 bytes.
pub fn parse_frame_len(header: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*header)
}

// ---------------------------------------------------------------------------
// Default KZ Castor tuning profile
// ---------------------------------------------------------------------------

impl DspConfig {
    /// Factory preset: KZ Castor — measured Harman-target correction.
    ///
    /// Measurements sourced from AutoEQ / squig.link community data.
    /// Bands ordered from low to high frequency.
    pub fn kz_castor_default() -> Self {
        Self {
            sample_rate: 48_000,
            quantum: 256,
            output_gain_db: -3.0,
            crossfeed: CrossfeedConfig {
                enabled: false,  // off by default — use HRTF instead
                cutoff_hz: 700.0,
                feed_level: 0.45,
                delay_samples: 14.4,
            },
            spatial: SpatialConfig {
                mode: SpatialMode::Hrtf,
                hrtf: HrtfConfig {
                    left_azimuth_deg:  -30.0,
                    right_azimuth_deg:  30.0,
                    elevation_deg:       0.0,
                    wet:                 1.0,
                    head_scale:          1.0,
                    auto_spin_hz:        0.0,
                    room_reverb:         0.2, // Touch of room feel by default
                },
            },
            peq: vec![
                // Band 1 – Sub-bass shelf boost
                PeqBand { freq:   55.0, gain_db:  3.5, q: 0.71, filter_type: FilterType::LowShelf,  enabled: true },
                // Band 2 – Mid-bass reduction
                PeqBand { freq:  200.0, gain_db: -2.5, q: 1.00, filter_type: FilterType::Peaking,   enabled: true },
                // Band 3 – Upper-bass bloom cut
                PeqBand { freq:  400.0, gain_db: -1.8, q: 1.20, filter_type: FilterType::Peaking,   enabled: true },
                // Band 4 – Lower-mid warmth
                PeqBand { freq:  900.0, gain_db:  1.0, q: 1.50, filter_type: FilterType::Peaking,   enabled: true },
                // Band 5 – Pinna shadow notch
                PeqBand { freq: 1_800.0, gain_db: -2.0, q: 2.00, filter_type: FilterType::Peaking,  enabled: true },
                // Band 6 – Presence boost (3–4 kHz Harman target)
                PeqBand { freq: 3_200.0, gain_db:  3.0, q: 1.20, filter_type: FilterType::Peaking,  enabled: true },
                // Band 7 – 5 kHz dip (KZ Castor energy spike)
                PeqBand { freq: 5_000.0, gain_db: -4.5, q: 2.50, filter_type: FilterType::Peaking,  enabled: true },
                // Band 8 – 8 kHz air boost
                PeqBand { freq: 8_000.0, gain_db:  2.5, q: 2.00, filter_type: FilterType::Peaking,  enabled: true },
                // Band 9 – 10 kHz BA harshness notch
                PeqBand { freq:10_000.0, gain_db: -3.0, q: 3.00, filter_type: FilterType::Peaking,  enabled: true },
                // Band 10 – Air shelf
                PeqBand { freq:16_000.0, gain_db:  2.0, q: 0.71, filter_type: FilterType::HighShelf, enabled: true },
            ],
        }
    }
}

//! `iem-dspd` — headless audio DSP daemon for KZ Castor IEM tuning.
//!
//! Architecture:
//! ```text
//!  ┌─────────────┐   ArcSwap<DspConfig>   ┌──────────────────────┐
//!  │  IPC Server │──────────────────────→ │  RT Audio Thread     │
//!  │  (tokio)    │                         │  PipeWire callback   │
//!  └─────────────┘                         │  10x Biquad PEQ      │
//!         ↑                                │  BS2B Crossfeed      │
//!         │ UnixDomainSocket               └──────────────────────┘
//!  ┌──────┴──────┐                                  │
//!  │  iem-ui     │           PipeWire Graph          │
//!  └─────────────┘   sink ←── DSP ←── apps ─────────┘
//! ```

mod dsp;
mod ipc;
mod pw;

use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use iem_common::DspConfig;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("iem-dspd")
        .join("config.toml")
}

fn load_config(path: &PathBuf) -> DspConfig {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
            error!("Config parse error: {e}. Using KZ Castor defaults.");
            DspConfig::kz_castor_default()
        }),
        Err(_) => {
            info!("No config at {path:?}. Writing KZ Castor defaults.");
            let cfg = DspConfig::kz_castor_default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, toml::to_string_pretty(&cfg).unwrap());
            cfg
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("iem_dspd=info".parse().unwrap()))
        .init();

    info!("iem-dspd starting — KZ Castor DSP daemon v{}", env!("CARGO_PKG_VERSION"));

    let cfg_path = config_path();
    let initial_cfg = load_config(&cfg_path);
    let shared_cfg: Arc<ArcSwap<DspConfig>> = Arc::new(ArcSwap::new(Arc::new(initial_cfg)));

    // IPC socket path
    let socket_path = PathBuf::from("/tmp/iem-dspd.sock");
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    // Build tokio runtime for IPC — does NOT touch the RT audio thread.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("iem-ipc")
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Spawn IPC server on the async runtime.
    let cfg_for_ipc = Arc::clone(&shared_cfg);
    let cfg_path_clone = cfg_path.clone();
    rt.spawn(async move {
        if let Err(e) = ipc::run_server(socket_path, cfg_for_ipc, cfg_path_clone).await {
            error!("IPC server error: {e}");
        }
    });

    // Launch PipeWire main loop + virtual sink.
    // This blocks the main thread (PW main loop runs here).
    pw::run_pipewire_main_loop(shared_cfg);

    info!("iem-dspd exiting.");
}

//! `ipc/mod.rs` — Async IPC server (tokio UnixDomainSocket).
//!
//! Protocol: 4-byte LE length-prefixed JSON frames.
//! Each connection is handled in its own task.

use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use iem_common::{parse_frame_len, encode_frame, DspConfig, IpcCommand, IpcResponse};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
use tracing::{error, info, warn};

/// Run the IPC server — binds the Unix socket and handles connections.
pub async fn run_server(
    socket_path: PathBuf,
    cfg: Arc<ArcSwap<DspConfig>>,
    cfg_disk_path: PathBuf,
) -> anyhow::Result<()> {
    let listener = UnixListener::bind(&socket_path)?;
    info!("IPC server listening at {:?}", socket_path);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let cfg = Arc::clone(&cfg);
                let path = cfg_disk_path.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, cfg, path).await {
                        warn!("IPC connection error: {e}");
                    }
                });
            }
            Err(e) => {
                error!("IPC accept error: {e}");
            }
        }
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    cfg: Arc<ArcSwap<DspConfig>>,
    cfg_disk_path: PathBuf,
) -> anyhow::Result<()> {
    loop {
        // ── Read 4-byte length header ──────────────────────────────────────
        let mut header = [0u8; 4];
        match stream.read_exact(&mut header).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }

        let len = parse_frame_len(&header) as usize;
        if len > 1_048_576 {
            warn!("IPC frame too large ({len} bytes), closing connection.");
            break;
        }

        // ── Read payload ───────────────────────────────────────────────────
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;

        let cmd: IpcCommand = match serde_json::from_slice(&payload) {
            Ok(c) => c,
            Err(e) => {
                let resp = IpcResponse::Error { message: format!("Parse error: {e}") };
                stream.write_all(&encode_frame(&resp)).await?;
                continue;
            }
        };

        // ── Dispatch command ───────────────────────────────────────────────
        let resp = dispatch(cmd, &cfg, &cfg_disk_path).await;

        // ── Send response ──────────────────────────────────────────────────
        stream.write_all(&encode_frame(&resp)).await?;
    }

    Ok(())
}

async fn dispatch(
    cmd: IpcCommand,
    cfg: &Arc<ArcSwap<DspConfig>>,
    disk_path: &PathBuf,
) -> IpcResponse {
    match cmd {
        IpcCommand::GetConfig => {
            let snap = cfg.load_full();
            IpcResponse::Config { config: (*snap).clone() }
        }

        IpcCommand::SetConfig { config } => {
            save_to_disk(&config, disk_path);
            cfg.store(Arc::new(config));
            IpcResponse::Ok
        }

        IpcCommand::SetBandEnabled { band, enabled } => {
            let mut new_cfg = (*cfg.load_full()).clone();
            if let Some(b) = new_cfg.peq.get_mut(band) {
                b.enabled = enabled;
                save_to_disk(&new_cfg, disk_path);
                cfg.store(Arc::new(new_cfg));
                IpcResponse::Ok
            } else {
                IpcResponse::Error { message: format!("Band {band} out of range") }
            }
        }

        IpcCommand::SetBandGain { band, gain_db } => {
            let mut new_cfg = (*cfg.load_full()).clone();
            if let Some(b) = new_cfg.peq.get_mut(band) {
                b.gain_db = gain_db;
                save_to_disk(&new_cfg, disk_path);
                cfg.store(Arc::new(new_cfg));
                IpcResponse::Ok
            } else {
                IpcResponse::Error { message: format!("Band {band} out of range") }
            }
        }

        IpcCommand::SetHeadYaw { yaw_deg } => {
            // Fast-path: update the atomic config but DO NOT write to disk.
            let mut new_cfg = (*cfg.load_full()).clone();
            new_cfg.spatial.hrtf.head_yaw_deg = yaw_deg;
            cfg.store(Arc::new(new_cfg));
            IpcResponse::Ok
        }

        IpcCommand::ReloadConfig => {
            match std::fs::read_to_string(disk_path) {
                Ok(s) => match toml::from_str::<DspConfig>(&s) {
                    Ok(new_cfg) => {
                        cfg.store(Arc::new(new_cfg));
                        IpcResponse::Ok
                    }
                    Err(e) => IpcResponse::Error { message: format!("TOML parse: {e}") },
                },
                Err(e) => IpcResponse::Error { message: format!("Read config: {e}") },
            }
        }

        IpcCommand::Shutdown => {
            info!("Shutdown requested via IPC.");
            std::process::exit(0);
        }
    }
}

fn save_to_disk(cfg: &DspConfig, path: &PathBuf) {
    if let Ok(s) = toml::to_string_pretty(cfg) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(path, s) {
            warn!("Failed to persist config: {e}");
        }
    }
}

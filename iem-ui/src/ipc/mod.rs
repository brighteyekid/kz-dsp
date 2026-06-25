//! IPC client: async send/receive over UnixStream.

use std::path::PathBuf;

use iem_common::{encode_frame, parse_frame_len, DspConfig, IpcCommand, IpcResponse};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Mutex,
};

pub struct IpcClient {
    stream: Mutex<UnixStream>,
}

impl IpcClient {
    pub async fn connect() -> anyhow::Result<Self> {
        let path = PathBuf::from("/tmp/iem-dspd.sock");
        let stream = UnixStream::connect(&path).await?;
        Ok(Self { stream: Mutex::new(stream) })
    }

    pub async fn send(&self, cmd: &IpcCommand) -> anyhow::Result<IpcResponse> {
        let frame = encode_frame(cmd);
        let mut stream = self.stream.lock().await;
        stream.write_all(&frame).await?;

        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await?;
        let len = parse_frame_len(&header) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;
        Ok(serde_json::from_slice(&payload)?)
    }

    pub async fn get_config(&self) -> anyhow::Result<DspConfig> {
        match self.send(&IpcCommand::GetConfig).await? {
            IpcResponse::Config { config } => Ok(config),
            IpcResponse::Error { message } => anyhow::bail!("Daemon error: {message}"),
            _ => anyhow::bail!("Unexpected IPC response"),
        }
    }

    pub async fn set_config(&self, cfg: &DspConfig) -> anyhow::Result<()> {
        let cmd = IpcCommand::SetConfig { config: cfg.clone() };
        match self.send(&cmd).await? {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected IPC response"),
        }
    }
}

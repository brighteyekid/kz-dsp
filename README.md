# iem-dspd — KZ Castor IEM DSP Daemon

High-performance, low-latency audio DSP pipeline engineered for the KZ Castor IEM.

![KZ DSP Logo](website/logo.png)

## 🏗 System Architecture

```mermaid
graph TD
    classDef daemon fill:#0A0A0A,stroke:#1AA88E,stroke-width:2px,color:#FFF,border-radius:8px;
    classDef cv fill:#0A0A0A,stroke:#C8A2FF,stroke-width:2px,color:#FFF,border-radius:8px;
    classDef ui fill:#0A0A0A,stroke:#555,stroke-width:2px,color:#FFF,border-radius:8px;
    classDef pw fill:#111,stroke:#333,stroke-width:1px,color:#DDD;

    subgraph "External Control"
        UI["iem-ui<br/>(Egui User Interface)"]:::ui
        CV["iem-tracker.py<br/>(OpenCV + MediaPipe)"]:::cv
    end

    subgraph "IEM-DSPD Background Daemon (Rust)"
        IPC["Fast-Path IPC<br/>(Unix Domain Sockets)"]:::daemon
        DSP["Real-Time DSP Engine<br/>(Woodworth HRTF + Freeverb)"]:::daemon
        PW["PipeWire Audio Server<br/>(Virtual Sink / Source)"]:::pw

        IPC ==>|Lock-Free Atomic Config Swap| DSP
        DSP <==>|Zero-Allocation Audio Stream| PW
    end

    UI -.->|JSON State Update| IPC
    CV ==>|Real-Time Head Yaw Data| IPC
```
## Core Features

1. **Bare-Metal PipeWire Integration:** Interfaces directly with PipeWire streams.
2. **Zero-Allocation Real-Time Thread:** All DSP algorithms operate on pre-allocated stack/heap memory without allocating during the audio callback, ensuring zero dropouts.
3. **Parametric Mastering:** 10-band zero-latency biquad PEQ.
4. **Immersive Spatial Audio:**
   * Woodworth Spherical-Head Model HRTF.
   * Internal LFO-driven auto-spin (orbital audio).
   * Freeverb FDN implementation for huge, reflection-rich room acoustics.
5. **Apple Vision Pro-Style World Lock:**
   * External Python script uses OpenCV and MediaPipe to calculate head yaw.
   * Feeds counter-rotation data directly to the Rust DSP fast-path via IPC Unix Socket (avoiding disk I/O).

## Documentation

See the [website documentation](website/doc/index.html) or `DOCS.md` for full installation, setup, and deployment instructions.

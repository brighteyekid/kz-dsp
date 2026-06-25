# iem-dspd — KZ Castor IEM DSP Daemon

High-performance, low-latency audio DSP pipeline engineered for the KZ Castor IEM.

![KZ DSP Logo](website/logo.png)

## Architecture

```
Computer Vision             iem-dspd (daemon)           iem-ui (GUI)
├── iem-tracker.py  ────┐   ├── pw/          ◄──────────► ipc/
│   (MediaPipe)         │   │   └── Virtual PipeWire     └── UnixStream JSON
│   30 FPS Head Yaw     │   │       Sink/Source
└───────────────────────┼──►├── dsp/
                        │   │   ├── biquad.rs   (10-Band PEQ)
                        │   │   ├── crossfeed.rs(BS2B)
                        │   │   ├── hrtf.rs     (Woodworth HRTF + Freeverb FDN)
                        │   │   └── engine.rs   (RT loop, lock-free atomics)
                        └──►└── ipc/
                                └── UnixDomainSocket server (Fast-Path)
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

# KZ Castor DSP - Complete Documentation

## Overview
KZ Castor DSP is a high-performance, ultra-low-latency Linux audio engine engineered specifically for the KZ Castor IEMs. It intercepts the default PipeWire sink to provide native Parametric EQ, BS2B Crossfeed, and a full 3D Spatial Audio suite including Woodworth HRTF and Freeverb FDN.

## Repository
**GitHub:** [https://github.com/brighteyekid/kz-dsp.git](https://github.com/brighteyekid/kz-dsp.git)

## Components

### 1. `iem-dspd` (The Daemon)
A headless background service written in pure Rust. 
*   **Zero-Allocation RT Path:** Memory is pre-allocated. The actual audio callback operates without relying on the OS memory allocator, ensuring no dropouts.
*   **Unix Domain Sockets IPC:** Receives config updates natively via `/tmp/iem-dspd.sock`. 

### 2. `iem-ui` (The GUI)
A lightweight Egui-based graphical interface for tuning parameters.
*   10-Band Biquad Filter interface.
*   BS2B cutoff/feed level control.
*   3D Spatial Audio pad (Radar interface).

### 3. `iem-tracker.py` (The Spatial Tracker)
A Python module utilizing `mediapipe.tasks.vision.FaceLandmarker` to estimate true 3D head yaw coordinates from a webcam.
*   Pushes coordinates at 30 FPS to the daemon.
*   Produces the "World Locked" Apple Vision Pro audio experience.

## Building and Deployment

### Dependencies
Ensure you have `cargo` and `pipewire` installed on Linux. For tracking, install Python 3.12, `opencv-python`, and `mediapipe`.

### Compilation
```bash
cargo build --release
cp target/release/iem-dspd ~/.cargo/bin/
cp target/release/iem-ui ~/.cargo/bin/
```

### Daemon Setup
The daemon relies on a `systemd` user service to persist across reboots.
```bash
systemctl --user start iem-dspd
systemctl --user enable iem-dspd
```

### Head Tracker Setup
```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python3 iem-tracker.py
```

## Spatial Audio Math (HRTF)
The engine utilizes a spherical-head model (Woodworth formula) combined with frequency-dependent IIR shelves and a pinna notch filter to synthesize externalized audio.
*   **ITD (Inter-aural Time Delay):** Delay line calculated based on the speed of sound and the virtual listener's head radius.
*   **ILD (Inter-aural Level Difference):** High-shelf filters simulate acoustic shadowing caused by the physical head.
*   **Auto-Spin:** An internal LFO orbits the virtual sound source 360 degrees around the listener's head, completely inside the zero-allocation path.

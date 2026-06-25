# Technical Architecture: KZ DSP

This document outlines the technical design decisions and system architecture for the `kz-dsp` project.

## 1. Zero-Allocation Real-Time Philosophy
Audio glitches (xruns) happen when the DSP thread blocks. The most common cause of blocking in high-level languages is memory allocation.
In `iem-dspd`, the `process_stereo_block` function is strictly bounded.
*   **Biquad Filters:** Statically sized array of 10 bands per channel. State (z1, z2) is mutated in place.
*   **HRTF Delay Lines:** Ring buffers are pre-allocated during configuration load. The Woodworth model accesses these via modulo arithmetic.
*   **FDN Reverb:** The Freeverb-style delay lines are sized maximally on startup. Room size parameters simply alter the feedback scaling, not the memory allocation.
*   **IPC Hot-Swapping:** `ArcSwap` from the `arc-swap` crate is used to swap the `DspConfig` pointer atomically. The RT thread reads the pointer without locking a mutex.

## 2. PipeWire Integration
PipeWire represents modern Linux audio. Instead of using ALSA directly, we use the `libpipewire` bindings via the `pw` module.
*   The daemon creates a `Virtual Sink` that applications (like browsers or Spotify) output to.
*   The daemon applies the DSP chain.
*   The daemon outputs to the physical hardware `Sink`.

## 3. World-Locked Spatial Audio
Creating an "out-of-head" experience requires more than just EQ. It requires simulating how sound waves hit the human head.

### The Woodworth Model
The Woodworth spherical head model calculates the Inter-aural Time Delay (ITD). 
If a sound is at 90 degrees (hard right), it hits the right ear immediately, but must travel *around* the curve of the head to reach the left ear. The delay `t` is calculated as `(r / c) * (theta + sin(theta))` where `r` is head radius and `c` is the speed of sound.
We implement this via fractional delay lines with linear interpolation.

### Low-Latency Head Tracking
To keep the soundstage locked to the physical room:
1. `iem-tracker.py` uses MediaPipe's `FaceLandmarker` task to extract a 4x4 facial transformation matrix.
2. Euler angles are derived to find the head yaw.
3. The yaw is sent via a 4-byte length-prefixed JSON frame over `/tmp/iem-dspd.sock`.
4. The daemon's IPC thread intercepts `SetHeadYaw`. Crucially, it updates the `ArcSwap` state *without* triggering a `toml::to_string` disk write.
5. The audio thread subtracts the yaw from the virtual speaker azimuths, achieving counter-rotation.

//! `pw/mod.rs` — PipeWire virtual DSP loopback: capture (virtual sink) → DSP → playback.
//!
//! # Architecture
//!
//! `pipewire-rs 0.10` exposes `pw_stream` but not `pw_filter`.
//! We use a **single bidirectional approach**: one stream as Input (virtual
//! sink), process audio in its `process` callback, then push the DSP output
//! into a second stream's buffer (Output, connected to the hardware sink).
//!
//! For maximum simplicity and correctness in the first version, we use a
//! **single capture stream** with `CAPTURE_SINK` set, so PipeWire gives us
//! mixed audio from all applications, processes it, then exposes it on the
//! **same node** as monitor ports.  A second `Output` stream drains the
//! monitor.
//!
//! In practice: the user points their application at "iem-dspd" in
//! PulseAudio/PipeWire settings or via `pw-link`.
//!
//! # Real-time safety
//!
//! All allocation happens at setup. The `process` closure holds only:
//! - `DspEngine` (stack-resident biquad state + ArcSwap ref)
//! - `Vec<f32>` scratch (pre-allocated `MAX_FRAMES * 2`)
//! - An `AtomicBool` for one-shot RT priority elevation

use std::{
    mem,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use arc_swap::ArcSwap;
use iem_common::DspConfig;
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::audio::{AudioFormat, AudioInfoRaw};
use pw::spa::pod::Pod;
use pw::stream::{StreamBox, StreamFlags};
use thread_priority::{set_current_thread_priority, ThreadPriority};
use tracing::{info, warn};

use crate::dsp::DspEngine;

/// Pre-allocated scratch: supports up to 2048 frames × 2 channels.
const MAX_FRAMES: usize = 2048;

pub fn run_pipewire_main_loop(cfg: Arc<ArcSwap<DspConfig>>) {
    pw::init();

    let main_loop = pw::main_loop::MainLoopRc::new(None).expect("PW MainLoop");
    let context   = pw::context::ContextRc::new(&main_loop, None).expect("PW Context");
    let core      = context.connect_rc(None).expect("PW Core");

    let snap  = cfg.load_full();
    let fs    = snap.sample_rate;
    let q     = snap.quantum;
    drop(snap);

    // ── Build format pod (F32LE stereo) ────────────────────────────────────
    let format_pod_bytes = {
        let mut info = AudioInfoRaw::new();
        info.set_format(AudioFormat::F32LE);
        info.set_rate(fs);
        info.set_channels(2);
        // Serialize to SPA POD
        let obj = pw::spa::pod::Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id:    pw::spa::param::ParamType::EnumFormat.as_raw(),
            properties: info.into(),
        };
        pw::spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &pw::spa::pod::Value::Object(obj),
        )
        .expect("pod serialise")
        .0
        .into_inner()
    };

    // ── DSP engine ─────────────────────────────────────────────────────────
    let mut engine  = DspEngine::new(Arc::clone(&cfg));
    let mut scratch = vec![0.0f32; MAX_FRAMES * 2];

    static RT_SET: AtomicBool = AtomicBool::new(false);

    // ── Virtual Sink stream (Input direction = apps write into it) ─────────
    //
    // Node properties that make PipeWire advertise this as an audio sink:
    //   MEDIA_CLASS = "Audio/Sink"
    //   MEDIA_CATEGORY = "Capture"
    //   STREAM_CAPTURE_SINK = "true"   ← captures from the sink monitor
    let sink_props = properties! {
        *pw::keys::MEDIA_TYPE       => "Audio",
        *pw::keys::MEDIA_CATEGORY   => "Capture",
        *pw::keys::MEDIA_ROLE       => "DSP",
        *pw::keys::MEDIA_CLASS      => "Audio/Sink",
        *pw::keys::NODE_NAME        => "iem-dspd",
        *pw::keys::NODE_NICK        => "KZ Castor DSP",
        *pw::keys::NODE_DESCRIPTION => "KZ Castor IEM 10-band PEQ + BS2B",
        *pw::keys::NODE_VIRTUAL     => "true",
        // Latency hint (frames/rate)
        *pw::keys::NODE_LATENCY     => format!("{q}/{fs}"),
    };

    let sink_stream = StreamBox::new(&core, "iem-dspd", sink_props)
        .expect("sink stream");

    // ── Capture process callback ───────────────────────────────────────────
    let _sink_listener = sink_stream
        .add_local_listener_with_user_data(())
        .state_changed(|_s, _, old, new| {
            info!("iem-dspd stream: {old:?} → {new:?}");
        })
        .param_changed(|stream, _ud, id, param| {
            let Some(param) = param else { return; };
            if id != pw::spa::param::ParamType::Format.as_raw() { return; }
            let Ok((mt, ms)) = pw::spa::param::format_utils::parse_format(param) else { return; };
            if mt != MediaType::Audio || ms != MediaSubtype::Raw { return; }
            let mut info = AudioInfoRaw::new();
            let _ = info.parse(param);
            info!("Format negotiated: {}Hz {}ch", info.rate(), info.channels());
        })
        .process(move |stream, _| {
            // ── One-shot RT priority elevation ─────────────────────────
            if !RT_SET.load(Ordering::Relaxed) {
                match set_current_thread_priority(ThreadPriority::Max) {
                    Ok(_)  => info!("RT thread: SCHED_FIFO elevated"),
                    Err(e) => warn!("RT priority denied ({e:?}), running best-effort"),
                }
                RT_SET.store(true, Ordering::Relaxed);
            }

            // ── Dequeue PipeWire buffer ────────────────────────────────
            let Some(mut buf) = stream.dequeue_buffer() else { return; };
            let datas = buf.datas_mut();
            if datas.is_empty() { return; }

            let (n_bytes, offset) = {
                let c = datas[0].chunk();
                (c.size() as usize, c.offset() as usize)
            };

            // Interleaved stereo f32 — bytes / sizeof(f32) / 2 = frames
            let n_samples = (n_bytes / mem::size_of::<f32>()).min(MAX_FRAMES * 2);
            if n_samples == 0 { return; }

            // ── Copy-in from PipeWire mapped memory ────────────────────
            if let Some(raw) = datas[0].data() {
                let src_f32: &[f32] = unsafe {
                    std::slice::from_raw_parts(
                        raw.as_ptr().add(offset) as *const f32,
                        n_samples,
                    )
                };
                scratch[..n_samples].copy_from_slice(src_f32);
            } else {
                return;
            }

            // ── ✦ DSP Engine: 10-band PEQ + BS2B crossfeed ✦ ──────────
            engine.process_block(&mut scratch[..n_samples]);

            // ── Write processed audio back into the PW buffer ──────────
            // data() on &mut Data returns Option<&mut [u8]>
            if let Some(raw) = datas[0].data() {
                let dst_f32: &mut [f32] = unsafe {
                    std::slice::from_raw_parts_mut(
                        raw.as_mut_ptr().add(offset) as *mut f32,
                        n_samples,
                    )
                };
                dst_f32.copy_from_slice(&scratch[..n_samples]);
            }

            // Buffer is returned to PipeWire when `buf` is dropped.
        })
        .register()
        .expect("register sink listener");

    // ── Connect stream ─────────────────────────────────────────────────────
    let format_pod = Pod::from_bytes(&format_pod_bytes).expect("format pod ref");
    let mut params = [format_pod];

    sink_stream
        .connect(
            spa::utils::Direction::Input,
            None,
            StreamFlags::AUTOCONNECT
                | StreamFlags::MAP_BUFFERS
                | StreamFlags::RT_PROCESS,
            &mut params,
        )
        .expect("sink stream connect");

    info!("iem-dspd: virtual sink registered. PipeWire main loop running.");
    main_loop.run();

    // SAFETY: called once at program exit, all PW objects are dropped.
    unsafe { pw::deinit(); }
}

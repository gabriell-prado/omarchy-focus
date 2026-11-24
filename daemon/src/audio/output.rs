//! Where the mixed audio goes: a PulseAudio stream (served by PipeWire's
//! pipewire-pulse on Omarchy) named "Focus", or a null sink that just keeps
//! real time, for tests.

use super::SAMPLE_RATE;
use std::thread;
use std::time::Duration;

pub trait Sink: Send {
    /// Write interleaved stereo f32 frames, blocking until there is room.
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkKind {
    Pulse,
    Null,
}

impl SinkKind {
    pub fn from_env() -> SinkKind {
        match std::env::var("OMARCHY_FOCUS_SINK").as_deref() {
            Ok("null") => SinkKind::Null,
            _ => SinkKind::Pulse,
        }
    }

    pub fn open(self) -> Result<Box<dyn Sink>, String> {
        match self {
            SinkKind::Pulse => PulseSink::open().map(|s| Box::new(s) as Box<dyn Sink>),
            SinkKind::Null => Ok(Box::new(NullSink)),
        }
    }
}

pub struct PulseSink {
    stream: libpulse_simple_binding::Simple,
    bytes: Vec<u8>,
}

/// Target buffer: small enough that changes are heard within ~60 ms.
const LATENCY_MS: u32 = 60;

impl PulseSink {
    pub fn open() -> Result<PulseSink, String> {
        use libpulse_binding::def::BufferAttr;
        use libpulse_binding::sample::{Format, Spec};
        use libpulse_binding::stream::Direction;

        let spec = Spec { format: Format::FLOAT32NE, channels: 2, rate: SAMPLE_RATE };
        let frame_bytes = 2 * 4;
        let attr = BufferAttr {
            maxlength: u32::MAX,
            tlength: SAMPLE_RATE / 1000 * LATENCY_MS * frame_bytes,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: u32::MAX,
        };
        let stream = libpulse_simple_binding::Simple::new(
            None,
            "Focus",
            Direction::Playback,
            None,
            "Focus sound",
            &spec,
            None,
            Some(&attr),
        )
        .map_err(|e| format!("cannot open audio output: {e}"))?;
        Ok(PulseSink { stream, bytes: Vec::new() })
    }
}

impl Sink for PulseSink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        self.bytes.clear();
        self.bytes.reserve(interleaved.len() * 4);
        for s in interleaved {
            self.bytes.extend_from_slice(&s.to_ne_bytes());
        }
        self.stream.write(&self.bytes).map_err(|e| format!("audio write failed: {e}"))
    }
}

/// Discards audio at real-time pace, so the engine behaves as with a device.
pub struct NullSink;

impl Sink for NullSink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        let frames = interleaved.len() as u64 / 2;
        thread::sleep(Duration::from_micros(frames * 1_000_000 / SAMPLE_RATE as u64));
        Ok(())
    }
}

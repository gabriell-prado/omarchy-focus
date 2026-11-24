//! Minimal 32-bit float stereo WAV writer, for offline renders.

use super::SAMPLE_RATE;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub fn write(path: &Path, interleaved: &[f32]) -> io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    let data_len = (interleaved.len() * 4) as u32;
    let byte_rate = SAMPLE_RATE * 2 * 4;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&3u16.to_le_bytes())?; // IEEE float
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&SAMPLE_RATE.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&8u16.to_le_bytes())?; // block align
    w.write_all(&32u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for s in interleaved {
        w.write_all(&s.to_le_bytes())?;
    }
    w.flush()
}

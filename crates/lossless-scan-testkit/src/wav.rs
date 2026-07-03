//! Minimal WAV writer for decode integration tests.

use std::io::Write;
use std::path::Path;

pub fn write_wav_f32_mono(path: &Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }

    let data_size = bytes.len() as u32;
    let file_size = 36 + data_size;
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * 2;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;
    f.write_all(&bytes)?;
    Ok(())
}

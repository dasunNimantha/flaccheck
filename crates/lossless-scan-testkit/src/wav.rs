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

/// Read 16-bit PCM WAV as mono f32 samples.
pub fn read_wav_f32_mono(path: &Path) -> std::io::Result<(Vec<f32>, u32)> {
    let data = std::fs::read(path)?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a WAV file",
        ));
    }
    let mut pos = 12usize;
    let mut sample_rate = 44100u32;
    let mut channels = 1u16;
    let mut bits = 16u16;
    let mut audio_offset = 0usize;
    let mut audio_len = 0usize;

    while pos + 8 <= data.len() {
        let chunk = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;
        if chunk == b"fmt " && size >= 16 {
            channels = u16::from_le_bytes(data[pos + 2..pos + 4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());
            bits = u16::from_le_bytes(data[pos + 14..pos + 16].try_into().unwrap());
        } else if chunk == b"data" {
            audio_offset = pos;
            audio_len = size;
            break;
        }
        pos += size + (size % 2);
    }

    if audio_len == 0 || bits != 16 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported WAV format",
        ));
    }

    let bytes = &data[audio_offset..audio_offset + audio_len.min(data.len() - audio_offset)];
    let frame_bytes = (bits / 8) as usize * channels as usize;
    let mut mono = Vec::with_capacity(bytes.len() / frame_bytes);
    for chunk in bytes.chunks(frame_bytes) {
        if chunk.len() < 2 {
            break;
        }
        let l = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
        if channels == 1 {
            mono.push(l);
        } else if chunk.len() >= 4 {
            let r = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
            mono.push((l + r) * 0.5);
        }
    }
    Ok((mono, sample_rate))
}

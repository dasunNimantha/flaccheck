use crate::DecodeError;
use lossless_scan_core::PcmBuffer;
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn decode(path: &Path) -> Result<PcmBuffer, DecodeError> {
  let src = File::open(path).map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;
  let mss = MediaSourceStream::new(Box::new(src), Default::default());

  let mut hint = Hint::new();
  if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
    hint.with_extension(ext);
  }

  let probed = symphonia::default::get_probe()
    .format(
      &hint,
      mss,
      &FormatOptions::default(),
      &MetadataOptions::default(),
    )
    .map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;

  let mut format = probed.format;
  let track = format
    .tracks()
    .iter()
    .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    .ok_or_else(|| DecodeError::DecodeFailed("no audio track".into()))?
    .clone();

  let mut decoder = symphonia::default::get_codecs()
    .make(&track.codec_params, &DecoderOptions::default())
    .map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;

  let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
  let channels = track
    .codec_params
    .channels
    .map(|c| c.count() as u16)
    .unwrap_or(2);
  let bits_per_sample = track.codec_params.bits_per_coded_sample.map(|b| b as u16);

  let mut samples: Vec<f32> = Vec::new();

  loop {
    let packet = match format.next_packet() {
      Ok(p) => p,
      Err(Error::ResetRequired) => continue,
      Err(Error::IoError(_)) => break,
      Err(e) => return Err(DecodeError::DecodeFailed(e.to_string())),
    };

    if packet.track_id() != track.id {
      continue;
    }

    let decoded = decoder
      .decode(&packet)
      .map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;

    append_samples(decoded, channels as usize, &mut samples);
  }

  if samples.is_empty() {
    return Err(DecodeError::DecodeFailed("no samples decoded".into()));
  }

  Ok(PcmBuffer {
    samples,
    sample_rate,
    channels,
    bits_per_sample,
  })
}

fn append_samples(decoded: AudioBufferRef<'_>, channels: usize, out: &mut Vec<f32>) {
  match decoded {
    AudioBufferRef::F32(buf) => {
      let l = buf.chan(0);
      let r = channel_f32(&buf, 1, l);
      interleave_f32(l, &r, channels, out);
    }
    AudioBufferRef::S16(buf) => {
      let l: Vec<f32> = buf.chan(0).iter().map(|&s| s as f32 / 32768.0).collect();
      let r = if buf.spec().channels.count() > 1 {
        buf.chan(1).iter().map(|&s| s as f32 / 32768.0).collect()
      } else {
        l.clone()
      };
      interleave_f32(&l, &r, channels, out);
    }
    AudioBufferRef::S32(buf) => {
      let l: Vec<f32> = buf
        .chan(0)
        .iter()
        .map(|&s| s as f32 / 2147483648.0)
        .collect();
      let r = if buf.spec().channels.count() > 1 {
        buf
          .chan(1)
          .iter()
          .map(|&s| s as f32 / 2147483648.0)
          .collect()
      } else {
        l.clone()
      };
      interleave_f32(&l, &r, channels, out);
    }
    AudioBufferRef::S24(buf) => {
      let l: Vec<f32> = buf.chan(0).iter().map(|s| i24_to_f32(*s)).collect();
      let r = if buf.spec().channels.count() > 1 {
        buf.chan(1).iter().map(|s| i24_to_f32(*s)).collect()
      } else {
        l.clone()
      };
      interleave_f32(&l, &r, channels, out);
    }
    _ => {}
  }
}

fn channel_f32(buf: &symphonia::core::audio::AudioBuffer<f32>, idx: usize, fallback: &[f32]) -> Vec<f32> {
  if buf.spec().channels.count() > idx {
    buf.chan(idx).to_vec()
  } else {
    fallback.to_vec()
  }
}

fn i24_to_f32(s: symphonia::core::sample::i24) -> f32 {
  let v = s.inner();
  v as f32 / 8388608.0
}

fn interleave_f32(left: &[f32], right: &[f32], channels: usize, out: &mut Vec<f32>) {
  let n = left.len().min(right.len());
  if channels == 1 {
    out.extend_from_slice(&left[..n]);
  } else {
    for i in 0..n {
      out.push(left[i]);
      out.push(right[i]);
    }
  }
}

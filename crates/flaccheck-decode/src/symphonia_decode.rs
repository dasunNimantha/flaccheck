use crate::DecodeError;
use flaccheck_core::PcmBuffer;
use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub fn decode(path: &Path) -> Result<PcmBuffer, DecodeError> {
    let src = File::open(path).map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| DecodeError::DecodeFailed("no audio track".into()))?;

    let audio_params = track
        .codec_params
        .as_ref()
        .ok_or_else(|| DecodeError::DecodeFailed("codec parameters missing".into()))?
        .audio()
        .ok_or_else(|| DecodeError::DecodeFailed("not an audio track".into()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|e| DecodeError::DecodeFailed(e.to_string()))?;

    let sample_rate = audio_params.sample_rate.unwrap_or(44100);
    let channels = audio_params
        .channels
        .as_ref()
        .map(|c| c.count() as u16)
        .unwrap_or(2);
    let bits_per_sample = audio_params.bits_per_sample.map(|b| b as u16);

    let track_id = track.id;
    let mut samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::ResetRequired) => continue,
            Err(Error::IoError(_)) => break,
            Err(e) => return Err(DecodeError::DecodeFailed(e.to_string())),
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(Error::DecodeError(_)) | Err(Error::IoError(_)) => continue,
            Err(e) => return Err(DecodeError::DecodeFailed(e.to_string())),
        };

        let n = decoded.samples_interleaved();
        let offset = samples.len();
        samples.resize(offset + n, 0.0);
        decoded.copy_to_slice_interleaved(&mut samples[offset..]);
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

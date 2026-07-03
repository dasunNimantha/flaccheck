//! Labeled test corpus — 60+ synthetic cases covering all detector tiers.

use crate::synth;
use lossless_scan_core::PcmBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroundTruth {
    Genuine,
    Transcoded,
    Inconclusive,
    NotGenuine,
}

#[derive(Debug, Clone)]
pub struct LabeledCase {
    pub id: &'static str,
    pub truth: GroundTruth,
    pub pcm: PcmBuffer,
    pub category: &'static str,
    /// Stretch goals — excluded from default pass-rate metrics.
    pub stretch: bool,
}

pub fn full_corpus() -> Vec<LabeledCase> {
    let mut cases = Vec::with_capacity(80);
    cases.extend(genuine_cases());
    cases.extend(transcode_cases());
    cases.extend(inconclusive_cases());
    cases.extend(hires_cases());
    cases
}

/// Core regression corpus (no stretch / known-gap cases).
pub fn core_corpus() -> Vec<LabeledCase> {
    full_corpus().into_iter().filter(|c| !c.stretch).collect()
}

fn genuine_cases() -> Vec<LabeledCase> {
    vec![
        case(
            "genuine_noise_1",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 1),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_7",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 7),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_42",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 42),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_99",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 99),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_1234",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 1234),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_5555",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 5555),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_9001",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 9001),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_31415",
            GroundTruth::Genuine,
            synth::wideband_noise(44100, 3.0, 31415),
            "genuine_wideband",
        ),
        case(
            "genuine_stereo_noise_a",
            GroundTruth::Genuine,
            {
                let l = synth::lcg_noise(44100 * 3, 2);
                let r = synth::lcg_noise(44100 * 3, 3);
                synth::stereo_from_mono(&l, &r)
            },
            "genuine_stereo",
        ),
        case(
            "genuine_stereo_noise_b",
            GroundTruth::Genuine,
            {
                let l = synth::lcg_noise(44100 * 3, 8);
                let r = synth::lcg_noise(44100 * 3, 9);
                synth::stereo_from_mono(&l, &r)
            },
            "genuine_stereo",
        ),
        case(
            "genuine_multitone_a",
            GroundTruth::Genuine,
            synth::multi_tone(44100, 3.0, &[440.0, 1200.0, 5000.0, 12000.0, 18000.0]),
            "genuine_multitone",
        ),
        case(
            "genuine_multitone_b",
            GroundTruth::Genuine,
            synth::multi_tone(
                44100,
                3.0,
                &[100.0, 800.0, 3500.0, 9000.0, 15000.0, 19000.0],
            ),
            "genuine_multitone",
        ),
        case(
            "genuine_multitone_c",
            GroundTruth::Genuine,
            synth::multi_tone(44100, 3.0, &[220.0, 1760.0, 7000.0, 14000.0]),
            "genuine_multitone",
        ),
        case(
            "genuine_96k_a",
            GroundTruth::Genuine,
            synth::wideband_noise(96000, 2.0, 11),
            "genuine_hires",
        ),
        case(
            "genuine_96k_b",
            GroundTruth::Genuine,
            synth::wideband_noise(96000, 2.0, 22),
            "genuine_hires",
        ),
        case(
            "genuine_gentle_rolloff_a",
            GroundTruth::Genuine,
            synth::gentle_rolloff_noise(44100, 3.0, 3),
            "genuine_analog_rolloff",
        ),
        case(
            "genuine_gentle_rolloff_b",
            GroundTruth::Genuine,
            synth::gentle_rolloff_noise(44100, 3.0, 31),
            "genuine_analog_rolloff",
        ),
        case(
            "genuine_gentle_rolloff_c",
            GroundTruth::Genuine,
            synth::gentle_rolloff_noise(44100, 3.0, 67),
            "genuine_analog_rolloff",
        ),
        case(
            "genuine_noise_48k_a",
            GroundTruth::Genuine,
            synth::wideband_noise(48000, 2.5, 77),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_48k_b",
            GroundTruth::Genuine,
            synth::wideband_noise(48000, 2.5, 88),
            "genuine_wideband",
        ),
        case(
            "genuine_noise_48k_c",
            GroundTruth::Genuine,
            synth::wideband_noise(48000, 2.5, 99),
            "genuine_wideband",
        ),
        case(
            "genuine_multitone_d",
            GroundTruth::Genuine,
            synth::multi_tone(48000, 2.5, &[300.0, 2500.0, 6000.0, 11000.0, 17000.0]),
            "genuine_multitone",
        ),
        case(
            "genuine_multitone_e",
            GroundTruth::Genuine,
            synth::multi_tone(44100, 2.5, &[55.0, 440.0, 3000.0, 10000.0, 20000.0]),
            "genuine_multitone",
        ),
        case(
            "genuine_96k_c",
            GroundTruth::Genuine,
            synth::wideband_noise(96000, 2.0, 33),
            "genuine_hires",
        ),
        case(
            "genuine_96k_d",
            GroundTruth::Genuine,
            synth::wideband_noise(96000, 2.0, 44),
            "genuine_hires",
        ),
    ]
}

fn transcode_cases() -> Vec<LabeledCase> {
    vec![
        case(
            "transcode_brick_11k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 11000.0, 100),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_12k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 12000.0, 101),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_14k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 14000.0, 102),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_16k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 16000.0, 103),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_17k5",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 17500.0, 104),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_18k5",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 18500.0, 105),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_19k5",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 19500.0, 106),
            "transcode_brick_wall",
        ),
        case(
            "transcode_tone_brick_16k",
            GroundTruth::Transcoded,
            {
                let base = synth::multi_tone(44100, 3.0, &[440.0, 2000.0, 8000.0, 15000.0]);
                synth::mono_buffer(
                    synth::brick_wall_hard(&base.samples, 44100, 16000.0),
                    44100,
                    16,
                )
            },
            "transcode_brick_wall",
        ),
        case(
            "transcode_tone_brick_19k",
            GroundTruth::Transcoded,
            {
                let base = synth::multi_tone(44100, 3.0, &[440.0, 2000.0, 8000.0, 15000.0]);
                synth::mono_buffer(
                    synth::brick_wall_hard(&base.samples, 44100, 19000.0),
                    44100,
                    16,
                )
            },
            "transcode_brick_wall",
        ),
        case(
            "transcode_heavy_lp_a",
            GroundTruth::NotGenuine,
            synth::mono_buffer(
                synth::steep_lowpass(&synth::lcg_noise(44100 * 3, 200), 0.05, 4),
                44100,
                16,
            ),
            "transcode_lowpass",
        ),
        case(
            "transcode_heavy_lp_b",
            GroundTruth::NotGenuine,
            synth::mono_buffer(
                synth::steep_lowpass(&synth::lcg_noise(44100 * 3, 201), 0.06, 4),
                44100,
                16,
            ),
            "transcode_lowpass",
        ),
        case(
            "transcode_heavy_lp_c",
            GroundTruth::NotGenuine,
            synth::mono_buffer(
                synth::steep_lowpass(&synth::lcg_noise(44100 * 3, 202), 0.07, 4),
                44100,
                16,
            ),
            "transcode_lowpass",
        ),
        case_stretch(
            "transcode_joint_stereo_a",
            GroundTruth::NotGenuine,
            synth::joint_stereo_collapsed(44100, 3.0, 10),
            "transcode_artifacts_stretch",
            true,
        ),
        case_stretch(
            "transcode_joint_stereo_b",
            GroundTruth::NotGenuine,
            synth::joint_stereo_collapsed(44100, 3.0, 20),
            "transcode_artifacts_stretch",
            true,
        ),
        case_stretch(
            "transcode_joint_stereo_c",
            GroundTruth::NotGenuine,
            synth::joint_stereo_collapsed(44100, 3.0, 30),
            "transcode_artifacts_stretch",
            true,
        ),
        case(
            "transcode_brick_13k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 13000.0, 107),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_10k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 10000.0, 111),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_9k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 9000.0, 112),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_8k5",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 8500.0, 113),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_15k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(44100, 3.0, 15000.0, 108),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_48k_18k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(48000, 2.5, 18000.0, 109),
            "transcode_brick_wall",
        ),
        case(
            "transcode_brick_48k_20k",
            GroundTruth::Transcoded,
            synth::fake_mp3_transcode(48000, 2.5, 20000.0, 110),
            "transcode_brick_wall",
        ),
        case(
            "transcode_heavy_lp_d",
            GroundTruth::NotGenuine,
            synth::mono_buffer(
                synth::steep_lowpass(&synth::lcg_noise(44100 * 3, 203), 0.08, 4),
                44100,
                16,
            ),
            "transcode_lowpass",
        ),
        case(
            "transcode_heavy_lp_e",
            GroundTruth::NotGenuine,
            synth::mono_buffer(
                synth::steep_lowpass(&synth::lcg_noise(44100 * 3, 204), 0.09, 4),
                44100,
                16,
            ),
            "transcode_lowpass",
        ),
    ]
}

fn inconclusive_cases() -> Vec<LabeledCase> {
    vec![
        case(
            "inconclusive_800hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 800.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_1500hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 1500.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_3000hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 3000.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_4500hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 4500.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_6000hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 6000.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_2000hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 2000.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_3500hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 3500.0),
            "inconclusive_band_limited",
        ),
        case(
            "inconclusive_7500hz",
            GroundTruth::Inconclusive,
            synth::pure_tone(44100, 3.0, 7500.0),
            "inconclusive_band_limited",
        ),
    ]
}

fn hires_cases() -> Vec<LabeledCase> {
    vec![
        case(
            "hires_padded_24_a",
            GroundTruth::NotGenuine,
            synth::padded_24bit_from_16(&synth::quantized_pattern(44100 * 2, 5)),
            "hires_padded",
        ),
        case(
            "hires_padded_24_b",
            GroundTruth::NotGenuine,
            synth::padded_24bit_from_16(&synth::quantized_pattern(44100 * 2, 15)),
            "hires_padded",
        ),
        case(
            "hires_padded_24_c",
            GroundTruth::NotGenuine,
            synth::padded_24bit_from_16(&synth::quantized_pattern(44100 * 2, 25)),
            "hires_padded",
        ),
        case(
            "hires_padded_24_d",
            GroundTruth::NotGenuine,
            synth::padded_24bit_from_16(&synth::quantized_pattern(44100 * 2, 35)),
            "hires_padded",
        ),
        case(
            "hires_upsampled_a",
            GroundTruth::NotGenuine,
            synth::fake_upsample_44k_to_96k(2.0, 6),
            "hires_upsampled",
        ),
        case(
            "hires_upsampled_b",
            GroundTruth::NotGenuine,
            synth::fake_upsample_44k_to_96k(2.0, 16),
            "hires_upsampled",
        ),
        case(
            "hires_upsampled_c",
            GroundTruth::NotGenuine,
            synth::fake_upsample_44k_to_96k(2.0, 26),
            "hires_upsampled",
        ),
        case(
            "hires_genuine_96k",
            GroundTruth::Genuine,
            synth::wideband_noise(96000, 2.5, 999),
            "hires_genuine",
        ),
        case(
            "hires_padded_24_e",
            GroundTruth::NotGenuine,
            synth::padded_24bit_from_16(&synth::quantized_pattern(44100 * 2, 45)),
            "hires_padded",
        ),
        case(
            "hires_upsampled_d",
            GroundTruth::NotGenuine,
            synth::fake_upsample_44k_to_96k(2.0, 36),
            "hires_upsampled",
        ),
        case(
            "hires_upsampled_e",
            GroundTruth::NotGenuine,
            synth::fake_upsample_44k_to_96k(2.5, 46),
            "hires_upsampled",
        ),
    ]
}

fn case(
    id: &'static str,
    truth: GroundTruth,
    pcm: PcmBuffer,
    category: &'static str,
) -> LabeledCase {
    case_stretch(id, truth, pcm, category, false)
}

fn case_stretch(
    id: &'static str,
    truth: GroundTruth,
    pcm: PcmBuffer,
    category: &'static str,
    stretch: bool,
) -> LabeledCase {
    LabeledCase {
        id,
        truth,
        pcm,
        category,
        stretch,
    }
}

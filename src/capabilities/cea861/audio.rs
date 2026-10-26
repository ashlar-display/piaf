use crate::model::prelude::Vec;
pub use display_types::cea861::audio::{
    AudioFormat, AudioFormatInfo, AudioSampleRates, ShortAudioDescriptor,
};

/// Parses an Audio Data Block payload into a list of [`ShortAudioDescriptor`]s.
///
/// `block_data` is the payload slice (excluding the header byte); any trailing bytes
/// that do not form a complete 3-byte group are silently ignored.
pub(super) fn parse_audio_data_block(block_data: &[u8]) -> Vec<ShortAudioDescriptor> {
    let mut out = Vec::new();
    let mut j = 0;
    while j + 3 <= block_data.len() {
        let b0 = block_data[j];
        let b1 = block_data[j + 1];
        let b2 = block_data[j + 2];

        let afc = (b0 >> 3) & 0x0F;
        let max_channels = (b0 & 0x07) + 1;
        let sample_rates = AudioSampleRates::from_bits_truncate(b1 & 0x7F);

        let format = match afc {
            1 => AudioFormat::Lpcm,
            2 => AudioFormat::Ac3,
            3 => AudioFormat::Mpeg1,
            4 => AudioFormat::Mp3,
            5 => AudioFormat::Mpeg2Multichannel,
            6 => AudioFormat::AacLc,
            7 => AudioFormat::Dts,
            8 => AudioFormat::Atrac,
            9 => AudioFormat::OneBitAudio,
            10 => AudioFormat::EnhancedAc3,
            11 => AudioFormat::DtsHd,
            12 => AudioFormat::MlpTrueHd,
            13 => AudioFormat::Dst,
            14 => AudioFormat::WmaPro,
            15 => AudioFormat::Extended((b2 >> 3) & 0x1F),
            _ => AudioFormat::Reserved(afc),
        };

        let format_info = match afc {
            1 => AudioFormatInfo::Lpcm {
                depth_16: b2 & 0x01 != 0,
                depth_20: b2 & 0x02 != 0,
                depth_24: b2 & 0x04 != 0,
            },
            2..=8 => AudioFormatInfo::MaxBitrateKbps((b2 as u16) * 8),
            _ => AudioFormatInfo::Raw(b2),
        };

        out.push(ShortAudioDescriptor::new(
            format,
            max_channels,
            sample_rates,
            format_info,
        ));
        j += 3;
    }
    out
}

/// Parses an HDMI Audio Data Block payload (after the extended tag byte), as edid-decode
/// (`cta_hdmi_audio_block`) and libdisplay-info (`parse_hdmi_audio_block`) decode it:
///
/// - byte 0: maximum stream count less one (bits 1:0), MS NonMixed support (bit 2);
/// - byte 1: number of 3D audio descriptors (bits 2:0);
/// - that many 4-byte 3D audio descriptors, then a 4-byte 3D speaker allocation.
///
/// Malformed content goes to `warn`: MS NonMixed with a single stream, more descriptors
/// claimed than the block holds (then none are taken, as libdisplay-info does), and a 3D
/// descriptor format other than L-PCM or One Bit Audio (that descriptor is skipped).
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn parse_hdmi_audio_block(
    payload: &[u8],
    warn: &mut dyn FnMut(crate::model::diagnostics::EdidWarning),
) -> display_types::cea861::HdmiAudioBlock {
    use crate::model::diagnostics::EdidWarning;
    use display_types::cea861::{
        Audio3dChannels, Audio3dSpeakerAllocation, HdmiAudioBlock, SpeakerAllocation,
        SpeakerAllocationFlags, SpeakerAllocationFlags2, SpeakerAllocationFlags3,
    };

    let first = payload.first().copied().unwrap_or(0);
    let max_streams = (first & 0x03) + 1;
    let ms_non_mixed = first & 0x04 != 0;
    if ms_non_mixed && max_streams == 1 {
        warn(EdidWarning::HdmiAudioNonMixedWithoutMultiStream);
    }
    let mut block = HdmiAudioBlock::new(max_streams, ms_non_mixed, Vec::new(), None);

    let claimed = payload.get(1).map_or(0, |&b| b & 0x07);
    if claimed == 0 {
        return block;
    }
    let descriptors = payload.get(2..).unwrap_or(&[]);
    // The 3D audio descriptors, then one speaker allocation descriptor.
    if (usize::from(claimed) + 1) * 4 > descriptors.len() {
        warn(EdidWarning::HdmiAudio3dDescriptorsExceedBlock {
            claimed,
            available_bytes: u8::try_from(descriptors.len()).unwrap_or(u8::MAX),
        });
        return block;
    }
    for d in descriptors.chunks_exact(4).take(usize::from(claimed)) {
        let code = d[0] & 0x0F;
        let (format, format_info) = match code {
            1 => (
                AudioFormat::Lpcm,
                AudioFormatInfo::Lpcm {
                    depth_16: d[3] & 0x01 != 0,
                    depth_20: d[3] & 0x02 != 0,
                    depth_24: d[3] & 0x04 != 0,
                },
            ),
            9 => (AudioFormat::OneBitAudio, AudioFormatInfo::Raw(d[3])),
            _ => {
                warn(EdidWarning::HdmiAudio3dUnsupportedFormat { code });
                continue;
            }
        };
        block.audio_3d_descriptors.push(ShortAudioDescriptor::new(
            format,
            (d[1] & 0x1F) + 1,
            AudioSampleRates::from_bits_truncate(d[2] & 0x7F),
            format_info,
        ));
    }
    let sa = &descriptors[usize::from(claimed) * 4..usize::from(claimed) * 4 + 4];
    let channels = match sa[3] >> 4 {
        1 => Audio3dChannels::Ch10_2,
        2 => Audio3dChannels::Ch22_2,
        3 => Audio3dChannels::Ch30_2,
        other => Audio3dChannels::Unknown(other),
    };
    block.speaker_allocation_3d = Some(Audio3dSpeakerAllocation::new(
        channels,
        SpeakerAllocation::new(
            SpeakerAllocationFlags::from_bits_truncate(sa[0]),
            SpeakerAllocationFlags2::from_bits_truncate(sa[1]),
            SpeakerAllocationFlags3::from_bits_truncate(sa[2]),
        ),
    ));
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lpcm_2ch_stereo() {
        // LPCM, 2 ch, 32/44.1/48 kHz, 16/20/24-bit
        // b0 = (1 << 3) | 1 = 0x09  (AFC=1, ch=2-1=1)
        // b1 = 0x07  (32+44.1+48 kHz)
        // b2 = 0x07  (16+20+24-bit)
        let sads = parse_audio_data_block(&[0x09, 0x07, 0x07]);
        assert_eq!(sads.len(), 1);
        let sad = &sads[0];
        assert_eq!(sad.format, AudioFormat::Lpcm);
        assert_eq!(sad.max_channels, 2);
        assert!(sad.sample_rates.contains(AudioSampleRates::HZ_32000));
        assert!(sad.sample_rates.contains(AudioSampleRates::HZ_44100));
        assert!(sad.sample_rates.contains(AudioSampleRates::HZ_48000));
        assert!(!sad.sample_rates.contains(AudioSampleRates::HZ_96000));
        assert_eq!(
            sad.format_info,
            AudioFormatInfo::Lpcm {
                depth_16: true,
                depth_20: true,
                depth_24: true
            }
        );
    }

    #[test]
    fn test_ac3_bitrate() {
        // AC-3, 6 ch, 48 kHz, max bitrate = 640 kbps → b2 = 640/8 = 80
        // b0 = (2 << 3) | 5 = 0x15  (AFC=2, ch=6-1=5)
        // b1 = 0x04  (48 kHz only)
        // b2 = 80
        let sads = parse_audio_data_block(&[0x15, 0x04, 80]);
        assert_eq!(sads.len(), 1);
        let sad = &sads[0];
        assert_eq!(sad.format, AudioFormat::Ac3);
        assert_eq!(sad.max_channels, 6);
        assert_eq!(sad.format_info, AudioFormatInfo::MaxBitrateKbps(640));
    }

    #[test]
    fn test_truehd_raw_byte3() {
        // MLP/TrueHD (AFC=12), ch=8, sample rate 192 kHz, byte3 raw
        // b0 = (12 << 3) | 7 = 0x67
        // b1 = 0x40  (192 kHz)
        // b2 = 0x00
        let sads = parse_audio_data_block(&[0x67, 0x40, 0x00]);
        assert_eq!(sads.len(), 1);
        let sad = &sads[0];
        assert_eq!(sad.format, AudioFormat::MlpTrueHd);
        assert_eq!(sad.max_channels, 8);
        assert!(sad.sample_rates.contains(AudioSampleRates::HZ_192000));
        assert_eq!(sad.format_info, AudioFormatInfo::Raw(0x00));
    }

    #[test]
    fn test_partial_trailing_bytes_ignored() {
        // 4 bytes: one complete SAD (3 bytes) + 1 trailing byte → only 1 SAD
        let sads = parse_audio_data_block(&[0x09, 0x07, 0x07, 0xFF]);
        assert_eq!(sads.len(), 1);
    }

    #[test]
    fn test_multiple_sads() {
        // Two SADs back-to-back
        let sads = parse_audio_data_block(&[
            0x09, 0x07, 0x07, // LPCM 2ch
            0x15, 0x04, 80, // AC-3 6ch
        ]);
        assert_eq!(sads.len(), 2);
        assert_eq!(sads[0].format, AudioFormat::Lpcm);
        assert_eq!(sads[1].format, AudioFormat::Ac3);
    }
}

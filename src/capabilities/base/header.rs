use crate::model::capabilities::DisplayCapabilities;
use crate::model::color::{
    AnalogColorType, Chromaticity, ColorBitDepth, DigitalColorEncoding, DisplayGamma,
};
use crate::model::edid::EdidVersion;
use crate::model::features::DisplayFeatureFlags;
use crate::model::input::{AnalogSyncLevel, VideoInputFlags, VideoInterface};
use crate::model::manufacture::{ManufactureDate, ManufacturerId};
use crate::model::screen::ScreenSize;

pub const fn decode_manufacture_date(week: u8, year: u8) -> ManufactureDate {
    let y = year as u16 + 1990;
    match week {
        0xFF => ManufactureDate::ModelYear(y),
        0x00 => ManufactureDate::Manufactured {
            week: None,
            year: y,
        },
        w => ManufactureDate::Manufactured {
            week: Some(w),
            year: y,
        },
    }
}

const fn decode_chromaticity(base: &[u8; 128]) -> Chromaticity {
    let lsb0 = base[0x19];
    let lsb1 = base[0x1A];
    Chromaticity {
        red: crate::model::color::ChromaticityPoint {
            x_raw: ((base[0x1B] as u16) << 2) | ((lsb0 >> 6) & 0x03) as u16,
            y_raw: ((base[0x1C] as u16) << 2) | ((lsb0 >> 4) & 0x03) as u16,
        },
        green: crate::model::color::ChromaticityPoint {
            x_raw: ((base[0x1D] as u16) << 2) | ((lsb0 >> 2) & 0x03) as u16,
            y_raw: ((base[0x1E] as u16) << 2) | (lsb0 & 0x03) as u16,
        },
        blue: crate::model::color::ChromaticityPoint {
            x_raw: ((base[0x1F] as u16) << 2) | ((lsb1 >> 6) & 0x03) as u16,
            y_raw: ((base[0x20] as u16) << 2) | ((lsb1 >> 4) & 0x03) as u16,
        },
        white: crate::model::color::ChromaticityPoint {
            x_raw: ((base[0x21] as u16) << 2) | ((lsb1 >> 2) & 0x03) as u16,
            y_raw: ((base[0x22] as u16) << 2) | (lsb1 & 0x03) as u16,
        },
    }
}

const fn decode_digital_color_encoding(bits: u8) -> DigitalColorEncoding {
    match (bits >> 3) & 0x03 {
        0b00 => DigitalColorEncoding::Rgb444,
        0b01 => DigitalColorEncoding::Rgb444YCbCr444,
        0b10 => DigitalColorEncoding::Rgb444YCbCr422,
        _ => DigitalColorEncoding::Rgb444YCbCr444YCbCr422,
    }
}

const fn decode_analog_color_type(bits: u8) -> Option<AnalogColorType> {
    match (bits >> 3) & 0x03 {
        0b00 => Some(AnalogColorType::Monochrome),
        0b01 => Some(AnalogColorType::Rgb),
        0b10 => Some(AnalogColorType::NonRgb),
        _ => None,
    }
}

pub const fn decode_color_bit_depth(bits: u8) -> Option<ColorBitDepth> {
    match bits & 0x07 {
        0b001 => Some(ColorBitDepth::Depth6),
        0b010 => Some(ColorBitDepth::Depth8),
        0b011 => Some(ColorBitDepth::Depth10),
        0b100 => Some(ColorBitDepth::Depth12),
        0b101 => Some(ColorBitDepth::Depth14),
        0b110 => Some(ColorBitDepth::Depth16),
        _ => None,
    }
}

const fn decode_video_interface(bits: u8) -> Option<VideoInterface> {
    match bits & 0x0F {
        0x1 => Some(VideoInterface::Dvi),
        0x2 => Some(VideoInterface::HdmiA),
        0x3 => Some(VideoInterface::HdmiB),
        0x4 => Some(VideoInterface::Mddi),
        0x5 => Some(VideoInterface::DisplayPort),
        _ => None,
    }
}

const fn decode_analog_sync_level(bits: u8) -> AnalogSyncLevel {
    match (bits >> 5) & 0x03 {
        0b00 => AnalogSyncLevel::V700_300,
        0b01 => AnalogSyncLevel::V714_286,
        0b10 => AnalogSyncLevel::V1000_400,
        _ => AnalogSyncLevel::V700_0,
    }
}

fn decode_screen_size(byte15: u8, byte16: u8) -> Option<ScreenSize> {
    match (byte15, byte16) {
        (0, 0) => None,
        (w, h) if w != 0 && h != 0 => Some(ScreenSize::Physical {
            width_cm: w,
            height_cm: h,
        }),
        (v, 0) => Some(ScreenSize::Landscape(v)),
        (0, v) => Some(ScreenSize::Portrait(v)),
        _ => unreachable!(),
    }
}

/// Decodes fixed-position header fields and returns `true` if the manufacturer ID was valid.
///
/// The caller is responsible for emitting [`EdidWarning::InvalidManufacturerId`][crate::EdidWarning]
/// when this returns `false`.
pub(super) fn decode_header_fields(base: &[u8; 128], caps: &mut DisplayCapabilities) -> bool {
    // Manufacturer ID (offsets 0x08-0x09)
    // 2 bytes, 3 characters, 5 bits per character (00001=A, ..., 11010=Z)
    let id_raw = ((base[0x08] as u16) << 8) | (base[0x09] as u16);
    let char1 = ((id_raw >> 10) & 0x1F) as u8;
    let char2 = ((id_raw >> 5) & 0x1F) as u8;
    let char3 = (id_raw & 0x1F) as u8;
    let manufacturer_valid =
        (1..=26).contains(&char1) && (1..=26).contains(&char2) && (1..=26).contains(&char3);
    if manufacturer_valid {
        caps.manufacturer = Some(ManufacturerId([
            char1 + b'A' - 1,
            char2 + b'A' - 1,
            char3 + b'A' - 1,
        ]));
    }

    // Product code (offsets 0x0A-0x0B, little-endian)
    let product_code = ((base[0x0B] as u16) << 8) | (base[0x0A] as u16);
    if product_code != 0 {
        caps.product_code = Some(product_code);
    }

    // Serial number (offsets 0x0C-0x0F, little-endian)
    let serial = ((base[0x0F] as u32) << 24)
        | ((base[0x0E] as u32) << 16)
        | ((base[0x0D] as u32) << 8)
        | (base[0x0C] as u32);
    if serial != 0 {
        caps.serial_number = Some(serial);
    }

    // Manufacture date (bytes 16-17)
    caps.manufacture_date = Some(decode_manufacture_date(base[16], base[17]));

    // EDID version and revision (bytes 18-19)
    caps.edid_version = Some(EdidVersion {
        version: base[18],
        revision: base[19],
    });

    // Chromaticity coordinates (bytes 0x19-0x22)
    caps.chromaticity = decode_chromaticity(base);

    // Display gamma (byte 0x17); 0xFF means undefined
    caps.gamma = DisplayGamma::from_edid_byte(base[0x17]);

    // Display feature support (byte 0x18)
    caps.display_features = Some(DisplayFeatureFlags::from_bits_truncate(base[0x18]));

    // Color type / encoding (byte 0x18 bits 4–3); meaning differs by input type and EDID version.
    // Digital encoding is only defined for EDID 1.4+; analog color type applies to any version.
    let is_digital = base[0x14] & 0x80 != 0;
    let edid_revision = base[19];
    if is_digital && edid_revision >= 4 {
        caps.digital_color_encoding = Some(decode_digital_color_encoding(base[0x18]));
    } else if !is_digital {
        caps.analog_color_type = decode_analog_color_type(base[0x18]);
    }

    // Video input definition (byte 0x14)
    let video_input = VideoInputFlags::from_bits_truncate(base[0x14]);
    caps.digital = video_input.contains(VideoInputFlags::DIGITAL);
    if caps.digital {
        caps.color_bit_depth = decode_color_bit_depth(base[0x14] >> 4);
        caps.video_interface = decode_video_interface(base[0x14]);
    } else {
        caps.analog_sync_level = Some(decode_analog_sync_level(base[0x14]));
    }

    // Screen size or aspect ratio (bytes 0x15-0x16)
    caps.screen_size = decode_screen_size(base[0x15], base[0x16]);

    manufacturer_valid
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::decode_chromaticity;
    use crate::capabilities::base::BaseBlockHandler;
    use crate::model::capabilities::DisplayCapabilities;
    use crate::model::color::{AnalogColorType, ColorBitDepth, DigitalColorEncoding, DisplayGamma};
    use crate::model::diagnostics::EdidWarning;
    use crate::model::edid::EdidVersion;
    use crate::model::extension::ExtensionHandler;
    use crate::model::features::DisplayFeatureFlags;
    use crate::model::input::{AnalogSyncLevel, VideoInterface};
    use crate::model::manufacture::{ManufactureDate, ManufacturerId};
    use crate::model::prelude::Vec;
    use crate::model::screen::ScreenSize;

    #[test]
    fn test_gamma() {
        let mut base = [0u8; 128];

        base[0x17] = 120; // (120 + 100) / 100 = 2.20
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.gamma, Some(DisplayGamma::from_edid_byte(120).unwrap()));
        assert!((caps.gamma.unwrap().value() - 2.20).abs() < 0.001);

        base[0x17] = 0xFF; // undefined
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.gamma, None);
    }

    #[test]
    fn test_edid_version() {
        let mut base = [0u8; 128];
        base[18] = 1;
        base[19] = 4;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.edid_version,
            Some(EdidVersion {
                version: 1,
                revision: 4
            })
        );
    }

    #[test]
    fn test_manufacture_date() {
        let mut base = [0u8; 128];

        // Week + year
        base[16] = 12;
        base[17] = 30; // 1990 + 30 = 2020
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::Manufactured {
                week: Some(12),
                year: 2020
            })
        );

        // Week unspecified
        base[16] = 0x00;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::Manufactured {
                week: None,
                year: 2020
            })
        );

        // Model year
        base[16] = 0xFF;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::ModelYear(2020))
        );
    }

    #[test]
    fn test_screen_size() {
        let mut base = [0u8; 128];

        // Physical dimensions
        base[0x15] = 60;
        base[0x16] = 34;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.screen_size,
            Some(ScreenSize::Physical {
                width_cm: 60,
                height_cm: 34
            })
        );

        // Landscape aspect ratio: byte 0x16 = 0, byte 0x15 = 196 → (196+99)/100 = 2.95
        base[0x15] = 196;
        base[0x16] = 0;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.screen_size, Some(ScreenSize::Landscape(196)));
        let ratio = caps.screen_size.unwrap().landscape_ratio().unwrap();
        assert!((ratio - 2.95).abs() < 0.001);

        // Portrait aspect ratio: byte 0x15 = 0, byte 0x16 = 101 → 100/(101+99) = 0.5
        base[0x15] = 0;
        base[0x16] = 101;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.screen_size, Some(ScreenSize::Portrait(101)));
        let ratio = caps.screen_size.unwrap().portrait_ratio().unwrap();
        assert!((ratio - 0.5).abs() < 0.001);

        // Both zero → undefined
        base[0x15] = 0;
        base[0x16] = 0;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.screen_size, None);
    }

    #[test]
    fn test_analog_sync_level() {
        let mut base = [0u8; 128];

        // Analog (bit 7 = 0), bits 6-5 = 0b00 → V700_300
        base[0x14] = 0x00;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_sync_level, Some(AnalogSyncLevel::V700_300));
        assert_eq!(caps.color_bit_depth, None);
        assert_eq!(caps.video_interface, None);

        // Analog, bits 6-5 = 0b01 → V714_286
        base[0x14] = 0x20;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_sync_level, Some(AnalogSyncLevel::V714_286));

        // Analog, bits 6-5 = 0b10 → V1000_400
        base[0x14] = 0x40;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_sync_level, Some(AnalogSyncLevel::V1000_400));

        // Analog, bits 6-5 = 0b11 → V700_0
        base[0x14] = 0x60;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_sync_level, Some(AnalogSyncLevel::V700_0));

        // Digital — sync level not populated
        base[0x14] = 0x80;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_sync_level, None);
    }

    #[test]
    fn test_color_bit_depth_and_video_interface() {
        let mut base = [0u8; 128];

        // Digital, 8 bpc (bits 6-4 = 0b010), DisplayPort (bits 3-0 = 0x5)
        base[0x14] = 0x80 | 0x25;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert!(caps.digital);
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth8));
        assert_eq!(caps.video_interface, Some(VideoInterface::DisplayPort));

        // Digital, undefined depth and interface
        base[0x14] = 0x80;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.color_bit_depth, None);
        assert_eq!(caps.video_interface, None);

        // Analog — neither field populated
        base[0x14] = 0x00;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert!(!caps.digital);
        assert_eq!(caps.color_bit_depth, None);
        assert_eq!(caps.video_interface, None);
    }

    #[test]
    fn test_chromaticity() {
        let mut base = [0u8; 128];

        base[0x1B] = (655u16 >> 2) as u8;
        base[0x1C] = (338u16 >> 2) as u8;
        base[0x1D] = (307u16 >> 2) as u8;
        base[0x1E] = (614u16 >> 2) as u8;
        base[0x1F] = (154u16 >> 2) as u8;
        base[0x20] = (61u16 >> 2) as u8;
        base[0x21] = (320u16 >> 2) as u8;
        base[0x22] = (337u16 >> 2) as u8;
        base[0x19] =
            (((655u16 & 3) << 6) | ((338u16 & 3) << 4) | ((307u16 & 3) << 2) | (614u16 & 3)) as u8;
        base[0x1A] = (((154u16 & 3) << 6) | ((61u16 & 3) << 4) | (337u16 & 3)) as u8;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.chromaticity, decode_chromaticity(&base));
        assert_eq!(caps.chromaticity.red.x_raw, 655);
        assert_eq!(caps.chromaticity.red.y_raw, 338);
        assert_eq!(caps.chromaticity.green.x_raw, 307);
        assert_eq!(caps.chromaticity.white.x_raw, 320);
        assert!((caps.chromaticity.red.x() - 0.640).abs() < 0.002);
        assert!((caps.chromaticity.white.x() - 0.3125).abs() < 0.002);
    }

    #[test]
    fn test_color_type() {
        let mut base = [0u8; 128];

        // Digital, EDID 1.4 (revision = 4), bits 4-3 = 0b11 → Rgb444YCbCr444YCbCr422
        base[0x14] = 0x80; // digital
        base[19] = 4; // revision 4
        base[0x18] = 0x18; // bits 4-3 = 0b11
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(
            caps.digital_color_encoding,
            Some(DigitalColorEncoding::Rgb444YCbCr444YCbCr422)
        );
        assert_eq!(caps.analog_color_type, None);

        // Digital, EDID 1.3 — encoding field not decoded
        base[19] = 3;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.digital_color_encoding, None);
        assert_eq!(caps.analog_color_type, None);

        // Analog, bits 4-3 = 0b01 → Rgb
        base[0x14] = 0x00; // analog
        base[0x18] = 0x08; // bits 4-3 = 0b01
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.digital_color_encoding, None);
        assert_eq!(caps.analog_color_type, Some(AnalogColorType::Rgb));

        // Analog, bits 4-3 = 0b11 → undefined (None)
        base[0x18] = 0x18;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        assert_eq!(caps.analog_color_type, None);
    }

    #[test]
    fn test_display_features() {
        let mut base = [0u8; 128];

        // DPMS standby + suspend + active-off + preferred timing = 0xE2
        base[0x18] = 0xE2;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        let flags = caps.display_features.unwrap();
        assert!(flags.contains(DisplayFeatureFlags::DPMS_STANDBY));
        assert!(flags.contains(DisplayFeatureFlags::DPMS_SUSPEND));
        assert!(flags.contains(DisplayFeatureFlags::DPMS_ACTIVE_OFF));
        assert!(flags.contains(DisplayFeatureFlags::PREFERRED_TIMING));
        assert!(!flags.contains(DisplayFeatureFlags::SRGB));
        assert!(!flags.contains(DisplayFeatureFlags::CONTINUOUS_TIMINGS));

        // sRGB + preferred timing + continuous timings = 0x07
        base[0x18] = 0x07;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        let flags = caps.display_features.unwrap();
        assert!(flags.contains(DisplayFeatureFlags::SRGB));
        assert!(flags.contains(DisplayFeatureFlags::PREFERRED_TIMING));
        assert!(flags.contains(DisplayFeatureFlags::CONTINUOUS_TIMINGS));
    }

    #[test]
    fn test_identification() {
        let mut base = [0u8; 128];

        // Manufacturer "SAM": S=19 (10011), A=1 (00001), M=13 (01101)
        // 0 10011 00001 01101 => 0x4C 0x2D
        base[0x08] = 0x4C;
        base[0x09] = 0x2D;

        // Product Code: 0x1234 (little-endian)
        base[0x0A] = 0x34;
        base[0x0B] = 0x12;

        // Serial Number: 0x12345678 (little-endian)
        base[0x0C] = 0x78;
        base[0x0D] = 0x56;
        base[0x0E] = 0x34;
        base[0x0F] = 0x12;

        // Video Input: Digital
        base[0x14] = 0x80;

        // Physical Dimensions: 51cm x 29cm
        base[0x15] = 51;
        base[0x16] = 29;

        // Monitor Name Descriptor at 0x36: "PIAF"
        base[0x36..0x3B].copy_from_slice(&[0x00, 0x00, 0x00, 0xFC, 0x00]);
        base[0x3B..0x3F].copy_from_slice(b"PIAF");
        base[0x3F] = 0x0A;
        base[0x40..0x48].fill(0x20);

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.manufacturer, Some(ManufacturerId(*b"SAM")));
        assert_eq!(caps.product_code, Some(0x1234));
        assert_eq!(caps.serial_number, Some(0x12345678));
        assert!(caps.digital);
        assert_eq!(
            caps.screen_size,
            Some(ScreenSize::Physical {
                width_cm: 51,
                height_cm: 29
            })
        );
        assert_eq!(caps.display_name.as_deref(), Some("PIAF"));
    }

    #[test]
    fn test_invalid_manufacturer_id_zero() {
        // 0x0000 → all three 5-bit chars are 0 (invalid)
        let mut base = [0u8; 128];
        base[0x08] = 0x00;
        base[0x09] = 0x00;
        let mut warnings = Vec::new();
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut warnings);
        assert_eq!(caps.manufacturer, None);
        assert!(
            warnings.iter().any(|w| (**w).downcast_ref::<EdidWarning>()
                == Some(&EdidWarning::InvalidManufacturerId))
        );
    }

    #[test]
    fn test_invalid_manufacturer_id_out_of_range() {
        // char values > 26 are invalid. Pack (27, 1, 1):
        // 0 11011 00001 00001 = 0x6C 0x21
        let mut base = [0u8; 128];
        base[0x08] = 0x6C;
        base[0x09] = 0x21;
        let mut warnings = Vec::new();
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut warnings);
        assert_eq!(caps.manufacturer, None);
        assert!(
            warnings.iter().any(|w| (**w).downcast_ref::<EdidWarning>()
                == Some(&EdidWarning::InvalidManufacturerId))
        );
    }
}

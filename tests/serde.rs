//! Serde round-trip tests for all public types that derive `Serialize`/`Deserialize`.
//!
//! Gated on the `serde` feature; run with:
//!
//! ```text
//! cargo test --features serde,std
//! ```
#![cfg(feature = "serde")]

use piaf::*;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

fn assert_round_trips<T: serde::Serialize + DeserializeOwned + PartialEq + Debug>(v: T) {
    let json = serde_json::to_string(&v).expect("serialize failed");
    let back: T = serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(v, back, "round-trip mismatch for JSON: {json}");
}

// ---------------------------------------------------------------------------
// EdidWarning
// ---------------------------------------------------------------------------

#[test]
fn edid_warning_simple_variants() {
    assert_round_trips(EdidWarning::UnknownExtension(0x02));
    assert_round_trips(EdidWarning::DescriptorParseFailed);
    assert_round_trips(EdidWarning::InvalidManufacturerId);
    assert_round_trips(EdidWarning::MalformedDataBlock);
    assert_round_trips(EdidWarning::DtdSlotTooShort);
    assert_round_trips(EdidWarning::DtdPixelClockOverflow);
    assert_round_trips(EdidWarning::DisplayIdVersionUnknown(0x20));
    assert_round_trips(EdidWarning::DisplayIdChecksumMismatch);
    assert_round_trips(EdidWarning::DisplayIdSectionBytesOutOfRange(200));
}

#[test]
fn edid_warning_struct_variants() {
    assert_round_trips(EdidWarning::HfEeodbBelowDeclared {
        declared: 3,
        hf_eeodb: 1,
    });
    assert_round_trips(EdidWarning::HfEeodbBlocksMissing {
        hf_eeodb: 3,
        present: 1,
    });
    assert_round_trips(EdidWarning::DisplayIdExtensionCountMismatch {
        declared: 2,
        found: 1,
    });
    assert_round_trips(EdidWarning::SizeMismatch {
        expected: 384,
        actual: 256,
    });
}

#[test]
fn read_warnings() {
    use piaf::read::ReadWarning;
    assert_round_trips(ReadWarning::ChecksumMismatch { block: 2 });
    assert_round_trips(ReadWarning::HfEeodbBelowDeclared {
        declared: 3,
        hf_eeodb: 1,
    });
    assert_round_trips(ReadWarning::BlocksNotRead {
        edid_blocks: 4,
        read: 2,
    });
}

// ---------------------------------------------------------------------------
// ManufactureDate
// ---------------------------------------------------------------------------

#[test]
fn manufacture_date_variants() {
    assert_round_trips(ManufactureDate::Manufactured {
        week: Some(14),
        year: 2023,
    });
    assert_round_trips(ManufactureDate::Manufactured {
        week: None,
        year: 2020,
    });
    assert_round_trips(ManufactureDate::ModelYear(2024));
}

// ---------------------------------------------------------------------------
// ScreenSize
// ---------------------------------------------------------------------------

#[test]
fn screen_size_variants() {
    assert_round_trips(ScreenSize::Physical {
        width_cm: 60,
        height_cm: 34,
    });
    assert_round_trips(ScreenSize::Landscape(60));
    assert_round_trips(ScreenSize::Portrait(40));
}

// ---------------------------------------------------------------------------
// VideoInterface / ColorBitDepth
// ---------------------------------------------------------------------------

#[test]
fn video_interface_variants() {
    assert_round_trips(VideoInterface::Dvi);
    assert_round_trips(VideoInterface::HdmiA);
    assert_round_trips(VideoInterface::HdmiB);
    assert_round_trips(VideoInterface::Mddi);
    assert_round_trips(VideoInterface::DisplayPort);
}

#[test]
fn color_bit_depth_variants() {
    assert_round_trips(ColorBitDepth::Depth6);
    assert_round_trips(ColorBitDepth::Depth8);
    assert_round_trips(ColorBitDepth::Depth10);
    assert_round_trips(ColorBitDepth::Depth12);
    assert_round_trips(ColorBitDepth::Depth14);
    assert_round_trips(ColorBitDepth::Depth16);
}

// ---------------------------------------------------------------------------
// panel.rs enums
// ---------------------------------------------------------------------------

#[test]
fn display_technology_variants() {
    assert_round_trips(DisplayTechnology::Tft);
    assert_round_trips(DisplayTechnology::DstnStn);
    assert_round_trips(DisplayTechnology::TftIps);
    assert_round_trips(DisplayTechnology::TftMva);
    assert_round_trips(DisplayTechnology::Crt);
    assert_round_trips(DisplayTechnology::Pdp);
    assert_round_trips(DisplayTechnology::Oled);
    assert_round_trips(DisplayTechnology::El);
    assert_round_trips(DisplayTechnology::FedSed);
    assert_round_trips(DisplayTechnology::Lcos);
    assert_round_trips(DisplayTechnology::Unknown(12));
}

#[test]
fn operating_mode_variants() {
    assert_round_trips(OperatingMode::Continuous);
    assert_round_trips(OperatingMode::NonContinuous);
    assert_round_trips(OperatingMode::Unknown(5));
}

#[test]
fn backlight_type_variants() {
    assert_round_trips(BacklightType::None);
    assert_round_trips(BacklightType::AcFluorescent);
    assert_round_trips(BacklightType::Dc);
    assert_round_trips(BacklightType::Unknown(3));
}

#[test]
fn physical_orientation_variants() {
    assert_round_trips(PhysicalOrientation::Landscape);
    assert_round_trips(PhysicalOrientation::Portrait);
    assert_round_trips(PhysicalOrientation::NotDefined);
    assert_round_trips(PhysicalOrientation::Undefined);
}

#[test]
fn rotation_capability_variants() {
    assert_round_trips(RotationCapability::None);
    assert_round_trips(RotationCapability::Cw90);
    assert_round_trips(RotationCapability::Deg180);
    assert_round_trips(RotationCapability::Cw270);
}

#[test]
fn zero_pixel_location_variants() {
    assert_round_trips(ZeroPixelLocation::UpperLeft);
    assert_round_trips(ZeroPixelLocation::UpperRight);
    assert_round_trips(ZeroPixelLocation::LowerLeft);
    assert_round_trips(ZeroPixelLocation::LowerRight);
}

#[test]
fn scan_direction_variants() {
    assert_round_trips(ScanDirection::NotDefined);
    assert_round_trips(ScanDirection::Normal);
    assert_round_trips(ScanDirection::Reversed);
    assert_round_trips(ScanDirection::Reserved);
}

#[test]
fn subpixel_layout_variants() {
    assert_round_trips(SubpixelLayout::NotDefined);
    assert_round_trips(SubpixelLayout::RgbVertical);
    assert_round_trips(SubpixelLayout::BgrVertical);
    assert_round_trips(SubpixelLayout::RgbHorizontal);
    assert_round_trips(SubpixelLayout::BgrHorizontal);
    assert_round_trips(SubpixelLayout::QuadRgbg);
    assert_round_trips(SubpixelLayout::QuadBgrg);
    assert_round_trips(SubpixelLayout::DeltaRgb);
    assert_round_trips(SubpixelLayout::DeltaBgr);
    assert_round_trips(SubpixelLayout::Unknown(0x42));
}

#[test]
fn display_interface_type_variants() {
    assert_round_trips(DisplayInterfaceType::Undefined);
    assert_round_trips(DisplayInterfaceType::Analog);
    assert_round_trips(DisplayInterfaceType::LvdsSingle);
    assert_round_trips(DisplayInterfaceType::LvdsDual);
    assert_round_trips(DisplayInterfaceType::TmdsSingle);
    assert_round_trips(DisplayInterfaceType::TmdsDual);
    assert_round_trips(DisplayInterfaceType::EmbeddedDisplayPort);
    assert_round_trips(DisplayInterfaceType::DisplayPort);
    assert_round_trips(DisplayInterfaceType::Proprietary);
    assert_round_trips(DisplayInterfaceType::Reserved(0xB));
}

#[test]
fn interface_content_protection_variants() {
    assert_round_trips(InterfaceContentProtection::None);
    assert_round_trips(InterfaceContentProtection::Hdcp);
    assert_round_trips(InterfaceContentProtection::Dpcp);
    assert_round_trips(InterfaceContentProtection::Reserved(3));
}

#[test]
fn tile_topology_behavior_variants() {
    assert_round_trips(TileTopologyBehavior::Undefined);
    assert_round_trips(TileTopologyBehavior::RequireAllTiles);
    assert_round_trips(TileTopologyBehavior::ScaleWhenMissing);
    assert_round_trips(TileTopologyBehavior::Reserved(3));
}

#[test]
fn stereo_viewing_mode_variants() {
    assert_round_trips(StereoViewingMode::FieldSequential);
    assert_round_trips(StereoViewingMode::SideBySide);
    assert_round_trips(StereoViewingMode::TopAndBottom);
    assert_round_trips(StereoViewingMode::RowInterleaved);
    assert_round_trips(StereoViewingMode::ColumnInterleaved);
    assert_round_trips(StereoViewingMode::PixelInterleaved);
    assert_round_trips(StereoViewingMode::Reserved(9));
}

#[test]
fn stereo_sync_interface_variants() {
    assert_round_trips(StereoSyncInterface::DisplayConnector);
    assert_round_trips(StereoSyncInterface::VesaDin);
    assert_round_trips(StereoSyncInterface::Infrared);
    assert_round_trips(StereoSyncInterface::RadioFrequency);
    assert_round_trips(StereoSyncInterface::Reserved(10));
}

// ---------------------------------------------------------------------------
// panel.rs structs (use from_str to work around #[non_exhaustive])
// ---------------------------------------------------------------------------

#[test]
fn power_sequencing_round_trip() {
    // #[non_exhaustive]: construct via serde_json to avoid struct literal restriction.
    let json = r#"{
        "t1_power_to_signal": 10,
        "t2_signal_to_backlight": 20,
        "t3_backlight_to_signal_off": 30,
        "t4_signal_to_power_off": 40,
        "t5_power_off_min": 50,
        "t6_backlight_off_min": 60
    }"#;
    let v: PowerSequencing = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

#[test]
fn display_id_interface_round_trip() {
    // #[non_exhaustive]: construct via serde_json to avoid struct literal restriction.
    let json = r#"{
        "interface_type": "EmbeddedDisplayPort",
        "spread_spectrum": true,
        "num_lanes": 4,
        "min_pixel_clock_10khz": 1000,
        "max_pixel_clock_10khz": 60000,
        "content_protection": "Hdcp"
    }"#;
    let v: DisplayIdInterface = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

#[test]
fn tile_bezel_info_round_trip() {
    let json = r#"{"top_px":5,"bottom_px":6,"right_px":7,"left_px":8}"#;
    let v: TileBezelInfo = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

#[test]
fn display_id_tiled_topology_round_trip() {
    let json = r#"{
        "single_enclosure": true,
        "topology_behavior": "RequireAllTiles",
        "h_tile_count": 2,
        "v_tile_count": 2,
        "h_tile_location": 0,
        "v_tile_location": 1,
        "tile_width_px": 1920,
        "tile_height_px": 1080,
        "bezel": {"top_px":3,"bottom_px":4,"right_px":5,"left_px":6}
    }"#;
    let v: DisplayIdTiledTopology = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

#[test]
fn display_id_tiled_topology_no_bezel() {
    let json = r#"{
        "single_enclosure": false,
        "topology_behavior": "Undefined",
        "h_tile_count": 1,
        "v_tile_count": 1,
        "h_tile_location": 0,
        "v_tile_location": 0,
        "tile_width_px": 3840,
        "tile_height_px": 2160,
        "bezel": null
    }"#;
    let v: DisplayIdTiledTopology = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

#[test]
fn display_id_stereo_interface_round_trip() {
    let json = r#"{
        "viewing_mode": "FieldSequential",
        "sync_polarity_positive": true,
        "sync_interface": "VesaDin"
    }"#;
    let v: DisplayIdStereoInterface = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

// ---------------------------------------------------------------------------
// transfer.rs
// ---------------------------------------------------------------------------

#[test]
fn transfer_point_encoding_variants() {
    assert_round_trips(TransferPointEncoding::Bits8);
    assert_round_trips(TransferPointEncoding::Bits10);
    assert_round_trips(TransferPointEncoding::Bits12);
}

#[test]
fn transfer_curve_luminance() {
    let v = TransferCurve::Luminance(vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_round_trips(v);
}

#[test]
fn transfer_curve_rgb() {
    let v = TransferCurve::Rgb {
        red: vec![0.0, 0.5, 1.0],
        green: vec![0.0, 0.4, 1.0],
        blue: vec![0.0, 0.6, 1.0],
    };
    assert_round_trips(v);
}

#[test]
fn display_id_transfer_characteristic_round_trip() {
    let json = r#"{
        "encoding": "Bits8",
        "curve": {"Luminance": [0.0, 0.5, 1.0]}
    }"#;
    let v: DisplayIdTransferCharacteristic = serde_json::from_str(json).expect("deserialize");
    assert_round_trips(v);
}

// ---------------------------------------------------------------------------
// Fixture-based DisplayCapabilities: spot-check that serde doesn't panic
// and key fields survive a JSON round-trip.
// ---------------------------------------------------------------------------

fn load(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("Failed to read fixture {path}: {e}"))
}

/// Verify that `DisplayCapabilities` serializes to valid JSON and round-trips without
/// losing scalar fields we can easily check via the raw Value.
///
/// `DisplayCapabilities` has no `PartialEq` (its `warnings` field contains
/// `Arc<dyn Error>`), so we inspect the parsed `Value` rather than comparing structs.
fn caps_roundtrip_value(fixture: &str) -> serde_json::Value {
    let bytes = load(fixture);
    let library = ExtensionLibrary::with_standard_handlers();
    let parsed = parse_edid(&bytes, &library).unwrap();
    let caps = capabilities_from_edid(&parsed, &library);

    let json = serde_json::to_string(&caps).expect("serialize caps");
    serde_json::from_str(&json).expect("deserialize caps as Value")
}

#[test]
fn lg_ultragear_caps_serde_round_trip() {
    let v = caps_roundtrip_value("testdata/valid/lg_ultragear_gsm.bin");
    // ManufacturerId serializes as a byte array [71, 83, 77] = "GSM".
    assert_eq!(v["manufacturer"], serde_json::json!([71, 83, 77]));
    // digital is a bool
    assert_eq!(v["digital"], serde_json::Value::Bool(true));
}

#[test]
fn philips_caps_serde_round_trip() {
    let v = caps_roundtrip_value("testdata/valid/phl_275e1_phl.bin");
    // Should deserialize without error and have a manufacturer field.
    assert!(v["manufacturer"].is_array());
}

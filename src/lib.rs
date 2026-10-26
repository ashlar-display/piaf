//! PIAF — a Rust library for reading and interpreting EDID display capability data.
//!
//! The core pipeline is:
//!
//! 1. [`parse_edid`] — validate and decode raw bytes into [`ParsedEdidRef`] (zero-copy).
//! 2. [`capabilities_from_edid`] — run extension handlers to produce [`DisplayCapabilities`].
//!
//! # Decoding philosophy
//!
//! PIAF decodes everything a specification defines. No field is omitted because it appears
//! obscure or unlikely to be needed — that judgement belongs to the consumer. [`Option`]
//! fields communicate presence or absence in the source data, not relative importance.
//!
//! # Quick start
//!
//! ```no_run
//! use piaf::{parse_edid, capabilities_from_edid, ExtensionLibrary};
//!
//! let bytes = std::fs::read("/sys/class/drm/card0-HDMI-A-1/edid").unwrap();
//! let library = ExtensionLibrary::with_standard_handlers();
//! let parsed = parse_edid(&bytes, &library).unwrap();
//! let caps = capabilities_from_edid(&parsed, &library);
//! println!("{:?}", caps.display_name);
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
// In a bare no_std build (no alloc, no std) the handler layer is absent, so the
// pub(crate) decode functions on model types appear unused. They are intentionally
// kept available for consumers who want to call them directly without the handler
// pipeline, so we suppress the warning rather than gating the items away.
#![cfg_attr(
    not(any(feature = "alloc", feature = "std")),
    allow(dead_code, unused_imports)
)]

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc;

/// Types for the EDID data model.
pub mod model;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use model::ExtensionData;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use model::ExtensionHandler;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use model::ParseWarning;
pub use model::{
    AnalogColorType, AnalogSyncLevel, BacklightType, Chromaticity, ChromaticityPoint,
    ColorBitDepth, ColorManagementData, CustomColorSpaceEotfCombo, CvtAlgorithm, CvtAspectRatio,
    CvtAspectRatios, CvtScaling, CvtSupportParams, DcmChannel, DigitalColorEncoding,
    DisplayCapabilities, DisplayFeatureFlags, DisplayGamma, DisplayIdInterface,
    DisplayIdStereoInterface, DisplayIdStereoInterfaceV2, DisplayIdTiledTopology,
    DisplayInterfaceFeatures, DisplayInterfaceType, DisplayTechnology, EdidError, EdidSource,
    EdidVersion, EdidWarning, ExtensionLibrary, ExtensionMetadata, ExtensionTagRegistry,
    GtfSecondaryParams, InterfaceContentProtection, KnownExtensions, MAX_STATIC_WARNINGS,
    ManufactureDate, ManufacturerId, ModeSink, MonitorString, OperatingMode, ParsedEdid,
    ParsedEdidRef, PhysicalOrientation, PowerSequencing, RefreshRate, RotationCapability,
    ScanDirection, ScreenSize, StaticContext, StaticDisplayCapabilities, StaticExtensionHandler,
    StereoMode, StereoSyncInterface, StereoTimingCode, StereoTimingCodeType, StereoTimingScopeV2,
    StereoViewingMode, SubpixelLayout, SyncDefinition, TileBezelInfo, TileTopologyBehavior,
    TimingFormula, TransferPointEncoding, TypeIxStereoMode, VideoInputFlags, VideoInterface,
    VideoMode, WhitePoint, ZeroPixelLocation,
};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use model::{DisplayIdTransferCharacteristic, TransferCurve};

/// EDID byte-level parser.
pub mod parser;
pub use parser::{parse_edid, parse_edid_owned};

/// Capability extraction from a [`ParsedEdid`].
pub mod capabilities;
pub use capabilities::Cea861Flags;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use capabilities::DisplayIdCapabilities;
pub use capabilities::capabilities_from_edid;
pub use capabilities::capabilities_from_edid_static;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use capabilities::{
    Audio3dChannels, Audio3dSpeakerAllocation, AudioFormat, AudioFormatInfo, AudioSampleRates,
    Cea861Capabilities, ColorimetryBlock, ColorimetryFlags, DtcPointEncoding, HdmiAudioBlock,
    HdmiDscMaxSlices, HdmiForumDsc, HdmiForumFrl, HdmiForumSinkCap, HdmiVsdb, HdmiVsdbFlags,
    HdrDynamicMetadataDescriptor, HdrEotf, HdrStaticMetadata, InfoFrameDescriptor,
    RoomConfigurationBlock, ShortAudioDescriptor, SpeakerAllocation, SpeakerAllocationFlags,
    SpeakerAllocationFlags2, SpeakerAllocationFlags3, SpeakerLocationEntry, T7VtdbBlock,
    T8VtdbBlock, T10VtdbBlock, T10VtdbEntry, VendorSpecificBlock, VesaDisplayDeviceBlock,
    VesaTransferCharacteristic, VideoCapability, VideoCapabilityFlags, VtbExtBlock, infoframe_type,
};
pub use capabilities::{
    CEA861_HANDLER, Cea861Handler, DISPLAYID_HANDLER, DisplayIdHandler, STANDARD_HANDLERS,
};

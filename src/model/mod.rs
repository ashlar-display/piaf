//! Data model types for parsed EDID data and display capabilities.

/// Re-exports of `alloc`/`std` collection types used across the crate.
pub mod prelude;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use prelude::{Box, String, Vec};

/// Extension handler traits and registries.
pub mod extension;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use extension::{ExtensionHandler, HandlerContext};
pub use extension::{
    ExtensionLibrary, ExtensionMetadata, ExtensionTagRegistry, KnownExtensions,
    StaticExtensionHandler,
};

/// Error and warning types.
pub mod diagnostics;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use diagnostics::ParseWarning;
pub use diagnostics::{EdidError, EdidWarning};

/// Decoding options.
pub mod options;
pub use options::{DecodeOptions, DtdBorderPolicy, DtdBorderReading};

/// Private warning storage.
pub(crate) mod warnings;

/// Parsed EDID intermediate representation.
pub mod edid;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use edid::ParsedEdid;
pub use edid::{EdidSource, EdidVersion, ParsedEdidRef};

/// Color-related model types.
pub mod color;
pub use color::{
    AnalogColorType, Chromaticity, ChromaticityPoint, ColorBitDepth, ColorManagementData,
    DcmChannel, DigitalColorEncoding, DisplayGamma, WhitePoint,
};

/// Input interface model types.
pub mod input;
pub use input::{AnalogSyncLevel, VideoInputFlags, VideoInterface};

/// Display feature flags.
pub mod features;
pub use features::DisplayFeatureFlags;

/// Manufacture date model type.
pub mod manufacture;
pub use manufacture::{ManufactureDate, ManufacturerId, MonitorString};

/// Screen size and aspect ratio.
pub mod screen;
pub use screen::ScreenSize;

/// Video timing formula types.
pub mod timing;
pub use timing::{
    CvtAspectRatio, CvtAspectRatios, CvtScaling, CvtSupportParams, GtfSecondaryParams,
    TimingFormula,
};

/// Panel hardware characteristic types (from DisplayID 0x0C Display Device Data Block).
pub mod panel;

/// Luminance transfer characteristic types (from DisplayID 0x0E Transfer Characteristics Block).
pub mod transfer;
pub use panel::{
    BacklightType, DisplayIdInterface, DisplayIdStereoInterface, DisplayIdTiledTopology,
    DisplayInterfaceType, DisplayTechnology, InterfaceContentProtection, OperatingMode,
    PhysicalOrientation, PowerSequencing, RotationCapability, ScanDirection, StereoSyncInterface,
    StereoViewingMode, SubpixelLayout, TileBezelInfo, TileTopologyBehavior, ZeroPixelLocation,
};
pub use transfer::TransferPointEncoding;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use transfer::{DisplayIdTransferCharacteristic, TransferCurve};

/// Consumer-facing capability types.
pub mod capabilities;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use capabilities::ExtensionData;
pub use capabilities::{
    CvtAlgorithm, DisplayCapabilities, MAX_STATIC_WARNINGS, ModeSink, RefreshRate, StaticContext,
    StaticDisplayCapabilities, StereoMode, SyncDefinition, TypeIxStereoMode, VideoMode,
};
pub use display_types::{
    CustomColorSpaceEotfCombo, DisplayIdStereoInterfaceV2, DisplayInterfaceFeatures,
    StereoTimingCode, StereoTimingCodeType, StereoTimingScopeV2,
};

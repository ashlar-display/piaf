use crate::model::diagnostics::EdidWarning;
use crate::model::manufacture::{ManufacturerId, MonitorString};
use crate::model::warnings::InlineWarnings;

pub use display_types::{
    CvtAlgorithm, ModeSource, RefreshRate, StereoMode, SyncDefinition, TypeIxStereoMode, VideoMode,
};
pub use display_types::{DisplayCapabilities, ExtensionData};

/// Sink for mode and warning writes from extension handlers.
///
/// Implemented by [`DisplayCapabilities`] (alloc builds, writes to `Vec`) and
/// `StaticDisplayCapabilities` (no_alloc builds, writes to a fixed array). Both handler traits —
/// [`ExtensionHandler`][crate::ExtensionHandler] and `StaticExtensionHandler` — write their output
/// through this trait so the core parsing logic is shared across build configurations.
pub trait ModeSink {
    /// Push a decoded video mode. Implementations ignore duplicates, but differ in what a
    /// duplicate is: [`DisplayCapabilities`] drops only a mode equal in every field (so two
    /// timings for the same resolution and rate are both kept), `StaticDisplayCapabilities`
    /// any mode with the same width, height, refresh rate and interlace flag (the first
    /// wins).
    fn push_mode(&mut self, mode: VideoMode);
    /// Push a non-fatal warning encountered while processing a block.
    fn push_warning(&mut self, w: EdidWarning);
}

/// Output context passed to [`StaticExtensionHandler::process`][crate::StaticExtensionHandler].
///
/// Handlers write decoded data through this type rather than directly to a sink trait object.
/// This allows the context to grow over time — additional sink types can be added as
/// `Option` fields without changing the handler trait signature.
///
/// Currently wraps a [`ModeSink`] for video mode and warning output. Future fields will
/// provide access to identity, colorimetry, and other sink types as the static pipeline
/// is extended.
pub struct StaticContext<'a> {
    modes: &'a mut dyn ModeSink,
}

impl<'a> StaticContext<'a> {
    /// Creates a new context backed by `modes`.
    pub fn new(modes: &'a mut dyn ModeSink) -> Self {
        Self { modes }
    }

    /// Pushes a decoded video mode. Duplicate modes are silently ignored.
    pub fn push_mode(&mut self, mode: VideoMode) {
        self.modes.push_mode(mode);
    }

    /// Pushes a non-fatal warning encountered while processing a block.
    pub fn push_warning(&mut self, w: EdidWarning) {
        self.modes.push_warning(w);
    }
}

impl ModeSink for StaticContext<'_> {
    fn push_mode(&mut self, mode: VideoMode) {
        self.modes.push_mode(mode);
    }

    fn push_warning(&mut self, w: EdidWarning) {
        self.modes.push_warning(w);
    }
}

/// `ModeSink` implementation for `DisplayCapabilities` (alloc/std builds only).
///
/// `push_mode` drops a mode equal in every field to one already present (full
/// `PartialEq`, so modes with the same resolution and rate but different timing details
/// are both kept). `push_warning` delegates to the inherent `push_warning` method from
/// display-types.
#[cfg(any(feature = "alloc", feature = "std"))]
impl ModeSink for DisplayCapabilities {
    fn push_mode(&mut self, mode: VideoMode) {
        if !self.supported_modes.contains(&mode) {
            self.supported_modes.push(mode);
        }
    }

    fn push_warning(&mut self, w: EdidWarning) {
        // Calls DisplayCapabilities::push_warning from display-types (inherent method).
        // Inherent methods take priority over trait methods in method resolution,
        // so this calls the display-types method, not this trait impl recursively.
        self.push_warning(w);
    }
}

/// No-alloc display capability model derived from a parsed EDID.
///
/// Contains the same scalar fields as [`DisplayCapabilities`] plus a fixed-capacity mode list.
/// Use `MAX_MODES` to set the maximum number of video modes that can be stored; 64 is a
/// reasonable default for most displays (real displays rarely declare more than ~40 modes).
/// Modes beyond the capacity are silently dropped. Warnings are capped at
/// [`MAX_STATIC_WARNINGS`]; those beyond it are counted in
/// [`warnings_dropped`][Self::warnings_dropped].
///
/// Produced by `capabilities_from_edid_static`.
///
/// # Stack size
///
/// At `MAX_MODES = 64` this struct is approximately 3 KB. On targets with very limited stack
/// space, consider placing the value in a `static mut` or a statically-allocated buffer
/// rather than on the stack.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct StaticDisplayCapabilities<const MAX_MODES: usize> {
    /// Three-character PNP manufacturer ID (e.g. `GSM` for LG, `SAM` for Samsung).
    pub manufacturer: Option<ManufacturerId>,
    /// Manufacture date or model year, decoded from bytes 16–17.
    pub manufacture_date: Option<crate::model::manufacture::ManufactureDate>,
    /// EDID specification version and revision, decoded from bytes 18–19.
    pub edid_version: Option<crate::model::edid::EdidVersion>,
    /// Manufacturer-assigned product code.
    pub product_code: Option<u16>,
    /// Manufacturer-assigned serial number, if encoded numerically in the base block.
    pub serial_number: Option<u32>,
    /// Serial number string from the monitor serial number descriptor (`0xFF`), if present.
    pub serial_number_string: Option<MonitorString>,
    /// Human-readable display name from the monitor name descriptor, if present.
    pub display_name: Option<MonitorString>,
    /// Unspecified ASCII text strings from `0xFE` descriptors, in descriptor slot order.
    ///
    /// Up to four entries (one per descriptor slot). Each slot is `None` if the corresponding
    /// descriptor was not a `0xFE` entry.
    pub unspecified_text: [Option<MonitorString>; 4],
    /// Additional white points from the `0xFB` descriptor.
    ///
    /// Up to two entries (the EDID `0xFB` descriptor has two fixed slots). Each slot is
    /// `None` if the corresponding entry was unused (index byte `0x00`).
    pub white_points: [Option<crate::model::color::WhitePoint>; 2],
    /// `true` if the display uses a digital input interface.
    pub digital: bool,
    /// Color bit depth per primary channel, decoded from byte `0x14` bits 6–4.
    /// `None` for analog displays or when the field is undefined or reserved.
    pub color_bit_depth: Option<crate::model::color::ColorBitDepth>,
    /// Physical display technology (e.g. TFT, OLED, PDP), from DisplayID 0x0C byte 0 bits 7:4.
    /// `None` when the Display Device Data Block is absent.
    pub display_technology: Option<crate::model::panel::DisplayTechnology>,
    /// Technology-specific sub-type code, from DisplayID 0x0C byte 0 bits 3:0 (raw, 0–15).
    /// `None` when the Display Device Data Block is absent.
    pub display_subtype: Option<u8>,
    /// Panel operating mode (continuous or non-continuous refresh), from DisplayID 0x0C byte 1 bits 3:0.
    /// `None` when the Display Device Data Block is absent.
    pub operating_mode: Option<crate::model::panel::OperatingMode>,
    /// Backlight type, from DisplayID 0x0C byte 1 bits 5:4.
    /// `None` when the Display Device Data Block is absent.
    pub backlight_type: Option<crate::model::panel::BacklightType>,
    /// Whether the panel uses a Data Enable (DE) signal, from DisplayID 0x0C byte 1 bit 6.
    /// `None` when the Display Device Data Block is absent.
    pub data_enable_used: Option<bool>,
    /// Data Enable signal polarity: `true` = positive, `false` = negative.
    /// From DisplayID 0x0C byte 1 bit 7. Valid only when `data_enable_used` is `Some(true)`.
    /// `None` when the Display Device Data Block is absent.
    pub data_enable_positive: Option<bool>,
    /// Native pixel format `(width_px, height_px)` from DisplayID 0x0C bytes 2–5.
    /// `None` when the Display Device Data Block is absent or either dimension is zero.
    pub native_pixels: Option<(u16, u16)>,
    /// Panel aspect ratio encoded as `(AR − 1) × 100` (raw byte), from DisplayID 0x0C byte 6.
    /// For example `77` represents approximately 16:9 (AR ≈ 1.77). `None` when the block is absent.
    pub panel_aspect_ratio_100: Option<u8>,
    /// Physical mounting orientation of the panel, from DisplayID 0x0C byte 7 bits 1:0.
    /// `None` when the Display Device Data Block is absent.
    pub physical_orientation: Option<crate::model::panel::PhysicalOrientation>,
    /// Panel rotation capability, from DisplayID 0x0C byte 7 bits 3:2.
    /// `None` when the Display Device Data Block is absent.
    pub rotation_capability: Option<crate::model::panel::RotationCapability>,
    /// Location of the zero (origin) pixel in the framebuffer, from DisplayID 0x0C byte 7 bits 5:4.
    /// `None` when the Display Device Data Block is absent.
    pub zero_pixel_location: Option<crate::model::panel::ZeroPixelLocation>,
    /// Fast-scan direction relative to H-sync, from DisplayID 0x0C byte 7 bits 7:6.
    /// `None` when the Display Device Data Block is absent.
    pub scan_direction: Option<crate::model::panel::ScanDirection>,
    /// Sub-pixel color filter arrangement, from DisplayID 0x0C byte 8.
    /// `None` when the Display Device Data Block is absent.
    pub subpixel_layout: Option<crate::model::panel::SubpixelLayout>,
    /// Pixel pitch `(horizontal_hundredths_mm, vertical_hundredths_mm)` in 0.01 mm units,
    /// from DisplayID 0x0C bytes 9–10. `None` when the Display Device Data Block is absent or
    /// either pitch is zero.
    pub pixel_pitch_hundredths_mm: Option<(u8, u8)>,
    /// Pixel response time in milliseconds, from DisplayID 0x0C byte 12.
    /// `None` when the Display Device Data Block is absent or the value is zero.
    pub pixel_response_time_ms: Option<u8>,
    /// Interface power sequencing timing parameters from DisplayID 0x0D.
    /// `None` when the Interface Power Sequencing Block is absent.
    pub power_sequencing: Option<crate::model::panel::PowerSequencing>,
    /// Physical display interface capabilities from DisplayID 0x0F.
    /// `None` when the Display Interface Data Block is absent.
    pub display_id_interface: Option<crate::model::panel::DisplayIdInterface>,
    /// Stereo display interface parameters from DisplayID 0x10.
    /// `None` when the Stereo Display Interface Data Block is absent.
    pub stereo_interface: Option<crate::model::panel::DisplayIdStereoInterface>,
    /// Tiled display topology from DisplayID 0x12.
    /// `None` when the Tiled Display Topology Data Block is absent.
    pub tiled_topology: Option<crate::model::panel::DisplayIdTiledTopology>,
    /// CIE xy chromaticity coordinates for the color primaries and white point,
    /// decoded from bytes `0x19`–`0x22`.
    pub chromaticity: crate::model::color::Chromaticity,
    /// Display gamma from byte `0x17`. `None` if the display did not specify a gamma value.
    pub gamma: Option<crate::model::color::DisplayGamma>,
    /// Display feature support flags from byte `0x18`.
    pub display_features: Option<crate::model::features::DisplayFeatureFlags>,
    /// Supported color encoding formats, decoded from byte `0x18` bits 4–3.
    /// Only populated for EDID 1.4+ digital displays.
    pub digital_color_encoding: Option<crate::model::color::DigitalColorEncoding>,
    /// Color type, decoded from byte `0x18` bits 4–3.
    /// Only populated for analog displays; `None` for the undefined value (`0b11`).
    pub analog_color_type: Option<crate::model::color::AnalogColorType>,
    /// Video interface type, decoded from byte `0x14` bits 3–0.
    /// `None` for analog displays or when the field is undefined or reserved.
    pub video_interface: Option<crate::model::input::VideoInterface>,
    /// Analog sync and video white levels, decoded from byte `0x14` bits 6–5.
    /// Only populated for analog displays.
    pub analog_sync_level: Option<crate::model::input::AnalogSyncLevel>,
    /// Physical screen dimensions or aspect ratio, decoded from bytes `0x15`–`0x16`.
    /// `None` when both bytes are zero (undefined).
    pub screen_size: Option<crate::model::screen::ScreenSize>,
    /// Minimum supported vertical refresh rate in Hz.
    pub min_v_rate: Option<u16>,
    /// Maximum supported vertical refresh rate in Hz.
    pub max_v_rate: Option<u16>,
    /// Minimum supported horizontal scan rate in kHz.
    pub min_h_rate_khz: Option<u16>,
    /// Maximum supported horizontal scan rate in kHz.
    pub max_h_rate_khz: Option<u16>,
    /// Maximum pixel clock in MHz.
    pub max_pixel_clock_mhz: Option<u16>,
    /// Physical image area dimensions in millimetres `(width_mm, height_mm)`, decoded from
    /// the first detailed timing descriptor that provides non-zero values (bytes 12–14).
    ///
    /// More precise than [`screen_size`][Self::screen_size] (which is in cm from the base
    /// block header). `None` when all DTD image-size fields are zero.
    pub preferred_image_size_mm: Option<(u16, u16)>,
    /// Video timing formula reported in the display range limits descriptor (`0xFD`), byte 10.
    pub timing_formula: Option<crate::model::timing::TimingFormula>,
    /// DCM polynomial coefficients decoded from a Color Management Data descriptor (`0xF9`).
    pub color_management: Option<crate::model::color::ColorManagementData>,
    /// Video modes decoded from the base block and extension handlers.
    ///
    /// Populated up to `MAX_MODES` entries. Use [`iter_modes`][Self::iter_modes] for a
    /// safe iterator that respects [`num_modes`][Self::num_modes].
    pub supported_modes: [Option<VideoMode>; MAX_MODES],
    /// Number of valid entries in [`supported_modes`][Self::supported_modes].
    pub num_modes: usize,
    // Non-fatal conditions collected from the parser and all handlers.
    warnings: InlineWarnings<MAX_STATIC_WARNINGS>,
}

/// The number of warnings a [`StaticDisplayCapabilities`] stores; further warnings are
/// counted in [`warnings_dropped`][StaticDisplayCapabilities::warnings_dropped].
pub const MAX_STATIC_WARNINGS: usize = 8;

impl<const MAX_MODES: usize> Default for StaticDisplayCapabilities<MAX_MODES> {
    fn default() -> Self {
        Self {
            manufacturer: None,
            manufacture_date: None,
            edid_version: None,
            product_code: None,
            serial_number: None,
            serial_number_string: None,
            display_name: None,
            unspecified_text: Default::default(),
            white_points: Default::default(),
            digital: false,
            color_bit_depth: None,
            display_technology: None,
            display_subtype: None,
            operating_mode: None,
            backlight_type: None,
            data_enable_used: None,
            data_enable_positive: None,
            native_pixels: None,
            panel_aspect_ratio_100: None,
            physical_orientation: None,
            rotation_capability: None,
            zero_pixel_location: None,
            scan_direction: None,
            subpixel_layout: None,
            pixel_pitch_hundredths_mm: None,
            pixel_response_time_ms: None,
            power_sequencing: None,
            display_id_interface: None,
            stereo_interface: None,
            tiled_topology: None,
            chromaticity: Default::default(),
            gamma: None,
            display_features: None,
            digital_color_encoding: None,
            analog_color_type: None,
            video_interface: None,
            analog_sync_level: None,
            screen_size: None,
            min_v_rate: None,
            max_v_rate: None,
            min_h_rate_khz: None,
            max_h_rate_khz: None,
            max_pixel_clock_mhz: None,
            preferred_image_size_mm: None,
            timing_formula: None,
            color_management: None,
            // `Option<VideoMode>` is not `Copy` so array repeat syntax is unavailable;
            // `core::array::from_fn` constructs each slot individually.
            supported_modes: core::array::from_fn(|_| None),
            num_modes: 0,
            warnings: InlineWarnings::new(),
        }
    }
}

impl<const MAX_MODES: usize> StaticDisplayCapabilities<MAX_MODES> {
    /// Returns an iterator over decoded video modes.
    pub fn iter_modes(&self) -> impl Iterator<Item = &VideoMode> {
        self.supported_modes[..self.num_modes].iter().flatten()
    }

    /// The collected warnings, in the order they occurred.
    pub fn warnings(&self) -> &[EdidWarning] {
        self.warnings.as_slice()
    }

    /// Returns an iterator over collected warnings, in the order they occurred.
    pub fn iter_warnings(&self) -> impl Iterator<Item = &EdidWarning> {
        self.warnings().iter()
    }

    /// The number of warnings that did not fit in the [`MAX_STATIC_WARNINGS`] slots
    /// (saturating at `u16::MAX`).
    pub fn warnings_dropped(&self) -> u16 {
        self.warnings.dropped()
    }
}

impl<const MAX_MODES: usize> ModeSink for StaticDisplayCapabilities<MAX_MODES> {
    fn push_mode(&mut self, mode: VideoMode) {
        if self.num_modes < MAX_MODES {
            // Dedup by (width, height, refresh_rate, interlaced) — the same criterion used
            // by the alloc pipeline when processing CEA SVDs.  The first entry for a given
            // resolution/rate wins, so a DTD-derived mode with full timing detail takes
            // precedence over a later SVD-derived mode with sparse timing detail.
            for i in 0..self.num_modes {
                if let Some(existing) = &self.supported_modes[i] {
                    if existing.width == mode.width
                        && existing.height == mode.height
                        && existing.refresh_rate == mode.refresh_rate
                        && existing.interlaced == mode.interlaced
                    {
                        return;
                    }
                }
            }
            self.supported_modes[self.num_modes] = Some(mode);
            self.num_modes += 1;
        }
        // Modes beyond capacity are silently dropped.
    }

    fn push_warning(&mut self, w: EdidWarning) {
        self.warnings.push(w);
    }
}

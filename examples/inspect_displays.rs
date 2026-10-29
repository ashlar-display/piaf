use piaf::{
    AudioFormat, AudioFormatInfo, Cea861Capabilities, Cea861Flags, Cea861Handler, ColorimetryFlags,
    DisplayCapabilities, DisplayIdCapabilities, DtcPointEncoding, ExtensionHandler,
    ExtensionLibrary, HdmiVsdbFlags, HdrEotf, ParseWarning, SpeakerAllocationFlags,
    SpeakerAllocationFlags2, SpeakerAllocationFlags3, TransferCurve, TransferPointEncoding,
    capabilities_from_edid, infoframe_type, parse_edid,
};
use std::fs;
use std::path::Path;

/// Renders the numeric portion of an `Option<RefreshRate>` for inline use in `{}@{}Hz`-style
/// format strings (no trailing "Hz"); `None` shows as `?`.
fn rate(r: Option<piaf::RefreshRate>) -> String {
    match r {
        Some(r) if r.denom() == 1 => r.numer().to_string(),
        Some(r) => format!("{}/{}", r.numer(), r.denom()),
        None => "?".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Custom extension data example
//
// This handler supersedes the built-in Cea861Handler for tag 0x02. It stores
// typed extension data that consumers can retrieve via `caps.get_extension_data`.
//
// Any type that is Debug + Send + Sync can be stored — no manual trait impl
// required beyond #[derive(Debug)].
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CeaDetails {
    version: u8,
    dtd_offset: u8,
}

#[derive(Debug)]
struct CeaDetailsHandler;

impl ExtensionHandler for CeaDetailsHandler {
    fn process(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
    ) {
        // Run the built-in handler first so VICs and modes are populated normally.
        Cea861Handler.process(blocks, caps, warnings);

        // Then overlay additional typed data under a custom key, sourced from the
        // first block (CEA-861 extension blocks are independent; extras are rare).
        if let Some(ext) = blocks.first() {
            caps.set_extension_data(
                0xF2,
                CeaDetails {
                    version: ext[1],
                    dtd_offset: ext[2],
                },
            );
        }
    }
}

fn main() {
    println!("--- Searching for connected displays (EDID) ---");

    let drm_path = Path::new("/sys/class/drm");
    if !drm_path.exists() {
        eprintln!("Error: /sys/class/drm not found. This script only works on Linux.");
        return;
    }

    // Start from the standard handler set, then replace the CEA-861 handler
    // with our custom one to capture additional typed data.
    let mut library = ExtensionLibrary::with_standard_handlers();
    if let Some(cea) = library.extensions.iter_mut().find(|e| e.tag == 0x02) {
        cea.handler = Some(Box::new(CeaDetailsHandler));
    }

    let mut found = 0;

    if let Ok(entries) = fs::read_dir(drm_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let edid_path = path.join("edid");
            if edid_path.exists() {
                if let Ok(bytes) = fs::read(&edid_path) {
                    if bytes.is_empty() || bytes.iter().all(|&b| b == 0) {
                        continue;
                    }

                    println!("\nFound connector: {}", name);
                    println!("EDID file: {}", edid_path.display());
                    println!("Data size: {} bytes", bytes.len());

                    match parse_edid(&bytes, &library) {
                        Ok(parsed) => {
                            let caps = capabilities_from_edid(&parsed, &library);

                            if parsed.num_extensions > 0 {
                                let ext_list: Vec<String> = (0..parsed.num_extensions)
                                    .filter_map(|i| parsed.extension_block(i))
                                    .map(|b| {
                                        let tag = b[0];
                                        let name = library
                                            .extensions
                                            .iter()
                                            .find(|e| e.tag == tag)
                                            .map(|e| e.display_name.as_str())
                                            .unwrap_or("Unknown");
                                        format!("{} (0x{:02X})", name, tag)
                                    })
                                    .collect();
                                println!("  Extensions:   {}", ext_list.join(", "));
                            }

                            if let Some(v) = caps.edid_version {
                                println!("  EDID version: {}", v);
                            }
                            println!(
                                "  Manufacturer: {:?}",
                                caps.manufacturer
                                    .as_ref()
                                    .map(|m| m.as_str())
                                    .unwrap_or("Unknown")
                            );
                            match caps.manufacture_date {
                                Some(piaf::ManufactureDate::Manufactured {
                                    week: Some(w),
                                    year,
                                }) => println!("  Manufactured: week {}, {}", w, year),
                                Some(piaf::ManufactureDate::Manufactured { week: None, year }) => {
                                    println!("  Manufactured: {}", year)
                                }
                                Some(piaf::ManufactureDate::ModelYear(year)) => {
                                    println!("  Model year:   {}", year)
                                }
                                Some(_) | None => {}
                            }
                            println!(
                                "  Display Name: {:?}",
                                caps.display_name.as_deref().unwrap_or("Unknown")
                            );
                            for text in caps.unspecified_text.iter().flatten() {
                                println!("  Info:         {}", text);
                            }
                            println!("  Product Code: {:?}", caps.product_code);
                            println!("  Serial:       {:?}", caps.serial_number);
                            if let Some(s) = caps.serial_number_string.as_deref() {
                                println!("  Serial string: {}", s);
                            }
                            match caps.screen_size {
                                Some(piaf::ScreenSize::Physical {
                                    width_cm,
                                    height_cm,
                                }) => println!("  Dimensions:   {}x{} cm", width_cm, height_cm),
                                Some(piaf::ScreenSize::Landscape(v)) => println!(
                                    "  Aspect ratio: {:.2}:1 (landscape)",
                                    (v as f32 + 99.0) / 100.0
                                ),
                                Some(piaf::ScreenSize::Portrait(v)) => println!(
                                    "  Aspect ratio: 1:{:.2} (portrait)",
                                    (v as f32 + 99.0) / 100.0
                                ),
                                Some(_) | None => {}
                            }
                            if let Some((w_mm, h_mm)) = caps.preferred_image_size_mm {
                                println!("  Image size:   {}x{} mm", w_mm, h_mm);
                            }
                            println!(
                                "  Input type:   {}",
                                if caps.digital { "Digital" } else { "Analog" }
                            );
                            if let Some(level) = caps.analog_sync_level {
                                println!("  Sync level:   {:?}", level);
                            }
                            if let Some(depth) = caps.color_bit_depth {
                                println!("  Color depth:  {} bpc", depth.bits_per_primary());
                            }
                            if let Some(gamma) = caps.gamma {
                                println!("  Gamma:        {:.2}", gamma.value());
                            }
                            let c = &caps.chromaticity;
                            println!(
                                "  Chromaticity: R({:.4},{:.4}) G({:.4},{:.4}) B({:.4},{:.4}) W({:.4},{:.4})",
                                c.red.x(),
                                c.red.y(),
                                c.green.x(),
                                c.green.y(),
                                c.blue.x(),
                                c.blue.y(),
                                c.white.x(),
                                c.white.y(),
                            );
                            if let Some(iface) = caps.video_interface {
                                println!("  Interface:    {:?}", iface);
                            }
                            if let Some(f) = caps.display_features {
                                println!("  Features:     {:?}", f);
                            }
                            if let Some(enc) = caps.digital_color_encoding {
                                println!("  Color enc:    {:?}", enc);
                            }
                            if let Some(ct) = caps.analog_color_type {
                                println!("  Color type:   {:?}", ct);
                            }
                            if let Some(tech) = caps.display_technology {
                                print!("  Technology:   {:?}", tech);
                                if let Some(sub) = caps.display_subtype {
                                    print!(" (subtype {})", sub);
                                }
                                println!();
                            }
                            if let Some(mode) = caps.operating_mode {
                                println!("  Op. mode:     {:?}", mode);
                            }
                            if let Some(bl) = caps.backlight_type {
                                println!("  Backlight:    {:?}", bl);
                            }
                            if let Some(px) = caps.native_pixels {
                                println!("  Native res:   {}x{} px", px.0, px.1);
                            }
                            if let Some(ar) = caps.panel_aspect_ratio_100 {
                                println!("  Panel AR:     {:.2}:1", (ar as f32 + 100.0) / 100.0);
                            }
                            if let Some(orient) = caps.physical_orientation {
                                println!("  Orientation:  {:?}", orient);
                            }
                            if let Some(rot) = caps.rotation_capability {
                                println!("  Rotation:     {:?}", rot);
                            }
                            if let Some(zp) = caps.zero_pixel_location {
                                println!("  Zero pixel:   {:?}", zp);
                            }
                            if let Some(sd) = caps.scan_direction {
                                println!("  Scan dir:     {:?}", sd);
                            }
                            if let Some(sp) = caps.subpixel_layout {
                                println!("  Subpixel:     {:?}", sp);
                            }
                            if let Some((hpitch, vpitch)) = caps.pixel_pitch_hundredths_mm {
                                println!(
                                    "  Pixel pitch:  {:.2} x {:.2} mm",
                                    hpitch as f32 / 100.0,
                                    vpitch as f32 / 100.0
                                );
                            }
                            if let Some(rt) = caps.pixel_response_time_ms {
                                println!("  Response:     {} ms", rt);
                            }
                            if let Some(true) = caps.data_enable_used {
                                println!(
                                    "  DE signal:    used, polarity={}",
                                    if caps.data_enable_positive.unwrap_or(false) {
                                        "positive"
                                    } else {
                                        "negative"
                                    }
                                );
                            }
                            if let Some(ps) = caps.power_sequencing {
                                println!(
                                    "  Power seq:    T1={}ms T2={}ms T3={}ms T4={}ms T5={}ms T6={}ms",
                                    ps.t1_power_to_signal as u16 * 2,
                                    ps.t2_signal_to_backlight as u16 * 2,
                                    ps.t3_backlight_to_signal_off as u16 * 2,
                                    ps.t4_signal_to_power_off as u16 * 2,
                                    ps.t5_power_off_min as u16 * 2,
                                    ps.t6_backlight_off_min as u16 * 2,
                                );
                            }
                            if let Some(tc) = &caps.transfer_characteristic {
                                let enc_str = match tc.encoding {
                                    TransferPointEncoding::Bits8 => "8-bit",
                                    TransferPointEncoding::Bits10 => "10-bit",
                                    TransferPointEncoding::Bits12 => "12-bit",
                                    _ => "unknown",
                                };
                                let point_info = match &tc.curve {
                                    TransferCurve::Luminance(v) => {
                                        format!("{} luminance points", v.len())
                                    }
                                    TransferCurve::Rgb { red, .. } => {
                                        format!("{} points/channel (RGB)", red.len())
                                    }
                                    _ => String::from("unknown"),
                                };
                                println!("  DisplayID TC: {} encoding, {}", enc_str, point_info);
                            }
                            if let Some(iface) = &caps.display_id_interface {
                                println!(
                                    "  DisplayID IF: {:?} lanes={} SS={} clk={}-{} kHz prot={:?}",
                                    iface.interface_type,
                                    iface.num_lanes,
                                    iface.spread_spectrum,
                                    iface.min_pixel_clock_10khz * 10,
                                    iface.max_pixel_clock_10khz * 10,
                                    iface.content_protection,
                                );
                            }
                            if let Some(stereo) = &caps.stereo_interface {
                                let polarity = if matches!(
                                    stereo.viewing_mode,
                                    piaf::StereoViewingMode::FieldSequential
                                ) {
                                    format!(
                                        " polarity={}",
                                        if stereo.sync_polarity_positive {
                                            "+"
                                        } else {
                                            "-"
                                        }
                                    )
                                } else {
                                    String::new()
                                };
                                println!(
                                    "  Stereo:       {:?} sync={:?}{}",
                                    stereo.viewing_mode, stereo.sync_interface, polarity
                                );
                            }
                            if let Some(topo) = &caps.tiled_topology {
                                let bezel_str = topo.bezel.map(|b| {
                                    format!(
                                        ", bezel T{}B{}L{}R{}px",
                                        b.top_px, b.bottom_px, b.left_px, b.right_px
                                    )
                                });
                                println!(
                                    "  Tiled:        {}x{} grid, tile [{},{}], {}x{}px{}{}",
                                    topo.h_tile_count,
                                    topo.v_tile_count,
                                    topo.h_tile_location,
                                    topo.v_tile_location,
                                    topo.tile_width_px,
                                    topo.tile_height_px,
                                    if topo.single_enclosure {
                                        ", single-enclosure"
                                    } else {
                                        ""
                                    },
                                    bezel_str.as_deref().unwrap_or(""),
                                );
                            }
                            if let Some(cea) = caps.get_extension_data::<Cea861Capabilities>(0x02) {
                                println!(
                                    "  Audio support: {}",
                                    if cea.flags.contains(Cea861Flags::BASIC_AUDIO) {
                                        "Yes"
                                    } else {
                                        "No"
                                    }
                                );
                                if !cea.vics.is_empty() {
                                    let vic_strs: Vec<String> = cea
                                        .vics
                                        .iter()
                                        .map(|(v, native)| {
                                            if *native {
                                                format!("{}*", v)
                                            } else {
                                                v.to_string()
                                            }
                                        })
                                        .collect();
                                    println!(
                                        "  CEA VICs ({}): {}",
                                        cea.vics.len(),
                                        vic_strs.join(", ")
                                    );
                                }
                                if !cea.audio_descriptors.is_empty() {
                                    println!(
                                        "  Audio descriptors ({}):",
                                        cea.audio_descriptors.len()
                                    );
                                    for sad in &cea.audio_descriptors {
                                        let fmt = match sad.format {
                                            AudioFormat::Lpcm => "LPCM",
                                            AudioFormat::Ac3 => "AC-3",
                                            AudioFormat::Mpeg1 => "MPEG-1",
                                            AudioFormat::Mp3 => "MP3",
                                            AudioFormat::Mpeg2Multichannel => "MPEG-2",
                                            AudioFormat::AacLc => "AAC-LC",
                                            AudioFormat::Dts => "DTS",
                                            AudioFormat::Atrac => "ATRAC",
                                            AudioFormat::OneBitAudio => "One-Bit",
                                            AudioFormat::EnhancedAc3 => "E-AC-3",
                                            AudioFormat::DtsHd => "DTS-HD",
                                            AudioFormat::MlpTrueHd => "TrueHD",
                                            AudioFormat::Dst => "DST",
                                            AudioFormat::WmaPro => "WMA-Pro",
                                            AudioFormat::Extended(_) => "Extended",
                                            AudioFormat::Reserved(_) => "Reserved",
                                            _ => "Unknown",
                                        };
                                        let info = match sad.format_info {
                                            AudioFormatInfo::Lpcm {
                                                depth_16,
                                                depth_20,
                                                depth_24,
                                            } => {
                                                let mut depths = Vec::new();
                                                if depth_16 {
                                                    depths.push("16-bit");
                                                }
                                                if depth_20 {
                                                    depths.push("20-bit");
                                                }
                                                if depth_24 {
                                                    depths.push("24-bit");
                                                }
                                                depths.join("/")
                                            }
                                            AudioFormatInfo::MaxBitrateKbps(kbps) => {
                                                format!("max {}kbps", kbps)
                                            }
                                            AudioFormatInfo::Raw(b) => format!("byte3=0x{:02X}", b),
                                            _ => String::from("unknown"),
                                        };
                                        println!("    {} {}ch  {}", fmt, sad.max_channels, info);
                                    }
                                }
                                if let Some(vsdb) = &cea.hdmi_vsdb {
                                    let spa = vsdb.source_physical_address;
                                    println!(
                                        "  HDMI SPA:     {}.{}.{}.{}",
                                        (spa >> 12) & 0xF,
                                        (spa >> 8) & 0xF,
                                        (spa >> 4) & 0xF,
                                        spa & 0xF,
                                    );
                                    if let Some(clk) = vsdb.max_tmds_clock_mhz {
                                        println!("  Max TMDS:     {} MHz", clk);
                                    }
                                    let mut dc = Vec::new();
                                    if vsdb.flags.contains(HdmiVsdbFlags::DC_30BIT) {
                                        dc.push("30-bit");
                                    }
                                    if vsdb.flags.contains(HdmiVsdbFlags::DC_36BIT) {
                                        dc.push("36-bit");
                                    }
                                    if vsdb.flags.contains(HdmiVsdbFlags::DC_48BIT) {
                                        dc.push("48-bit");
                                    }
                                    if vsdb.flags.contains(HdmiVsdbFlags::DC_Y444) {
                                        dc.push("Y444");
                                    }
                                    if !dc.is_empty() {
                                        println!("  Deep color:   {}", dc.join(", "));
                                    }
                                }
                                if let Some(vc) = &cea.video_capability {
                                    println!(
                                        "  Video cap:    QS={} QY={} PT={} IT={} CE={}",
                                        vc.flags.contains(piaf::VideoCapabilityFlags::QS) as u8,
                                        vc.flags.contains(piaf::VideoCapabilityFlags::QY) as u8,
                                        vc.pt_behaviour,
                                        vc.it_behaviour,
                                        vc.ce_behaviour,
                                    );
                                }
                                if let Some(cm) = &cea.colorimetry {
                                    let mut standards = Vec::new();
                                    if cm.colorimetry.contains(ColorimetryFlags::XVYCC601) {
                                        standards.push("xvYCC601");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::XVYCC709) {
                                        standards.push("xvYCC709");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::SYCC601) {
                                        standards.push("sYCC601");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::OPYCC601) {
                                        standards.push("opYCC601");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::OPRGB) {
                                        standards.push("opRGB");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::BT2020CYCC) {
                                        standards.push("BT.2020cYCC");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::BT2020YCC) {
                                        standards.push("BT.2020YCC");
                                    }
                                    if cm.colorimetry.contains(ColorimetryFlags::BT2020RGB) {
                                        standards.push("BT.2020RGB");
                                    }
                                    if !standards.is_empty() {
                                        println!("  Colorimetry:  {}", standards.join(", "));
                                    }
                                }
                                if let Some(hdr) = &cea.hdr_static_metadata {
                                    let mut eotfs = Vec::new();
                                    if hdr.eotf.contains(HdrEotf::SDR) {
                                        eotfs.push("SDR");
                                    }
                                    if hdr.eotf.contains(HdrEotf::HDR) {
                                        eotfs.push("HDR");
                                    }
                                    if hdr.eotf.contains(HdrEotf::ST2084) {
                                        eotfs.push("PQ/HDR10");
                                    }
                                    if hdr.eotf.contains(HdrEotf::HLG) {
                                        eotfs.push("HLG");
                                    }
                                    println!("  HDR EOTFs:    {}", eotfs.join(", "));
                                    if let Some(max) = hdr.max_luminance {
                                        println!("  Max lum:      {:.0} cd/m²", max);
                                    }
                                    if let Some(min) = hdr.min_luminance {
                                        println!("  Min lum:      {:.4} cd/m²", min);
                                    }
                                    if let Some(fall) = hdr.max_fall {
                                        println!("  MaxFALL:      {:.0} cd/m²", fall);
                                    }
                                }
                                if let Some(sa) = &cea.speaker_allocation {
                                    let mut ch = Vec::new();
                                    if sa.channels.contains(SpeakerAllocationFlags::FL_FR) {
                                        ch.push("FL/FR");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::LFE1) {
                                        ch.push("LFE1");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::FC) {
                                        ch.push("FC");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::BL_BR) {
                                        ch.push("BL/BR");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::BC) {
                                        ch.push("BC");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::FLC_FRC) {
                                        ch.push("FLC/FRC");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::RLC_RRC) {
                                        ch.push("RLC/RRC");
                                    }
                                    if sa.channels.contains(SpeakerAllocationFlags::FLW_FRW) {
                                        ch.push("FLW/FRW");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::TP_FL_FR) {
                                        ch.push("TpFL/TpFR");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::TP_C) {
                                        ch.push("TpC");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::TP_FC) {
                                        ch.push("TpFC");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::LS_RS) {
                                        ch.push("LS/RS");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::LFE2) {
                                        ch.push("LFE2");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::TP_BC) {
                                        ch.push("TpBC");
                                    }
                                    if sa.channels_2.contains(SpeakerAllocationFlags2::SI_L_SI_R) {
                                        ch.push("SiL/SiR");
                                    }
                                    if sa
                                        .channels_2
                                        .contains(SpeakerAllocationFlags2::TP_SI_L_TP_SI_R)
                                    {
                                        ch.push("TpSiL/TpSiR");
                                    }
                                    if sa.channels_3.contains(SpeakerAllocationFlags3::TP_BL_TP_BR)
                                    {
                                        ch.push("TpBL/TpBR");
                                    }
                                    if sa.channels_3.contains(SpeakerAllocationFlags3::BT_FC) {
                                        ch.push("BtFC");
                                    }
                                    if sa.channels_3.contains(SpeakerAllocationFlags3::BT_FL_BT_FR)
                                    {
                                        ch.push("BtFL/BtFR");
                                    }
                                    if sa.channels_3.contains(SpeakerAllocationFlags3::TP_LS_TP_RS)
                                    {
                                        ch.push("TpLS/TpRS");
                                    }
                                    if !ch.is_empty() {
                                        println!("  Speakers:     {}", ch.join(", "));
                                    }
                                }
                                if let Some(dtc) = &cea.vesa_transfer_characteristic {
                                    let enc = match dtc.encoding {
                                        DtcPointEncoding::Bits8 => "8-bit",
                                        DtcPointEncoding::Bits10 => "10-bit",
                                        DtcPointEncoding::Bits12 => "12-bit",
                                        _ => "unknown",
                                    };
                                    println!(
                                        "  VESA DTC:     {} encoding, {} points",
                                        enc,
                                        dtc.points.len()
                                    );
                                }
                                if !cea.hdr_dynamic_metadata.is_empty() {
                                    println!(
                                        "  HDR Dynamic Metadata ({}):",
                                        cea.hdr_dynamic_metadata.len()
                                    );
                                    for d in &cea.hdr_dynamic_metadata {
                                        println!(
                                            "    type={} version={}",
                                            d.application_type, d.application_version
                                        );
                                    }
                                }
                                if !cea.video_format_preferences.is_empty() {
                                    let svrs: Vec<String> = cea
                                        .video_format_preferences
                                        .iter()
                                        .map(|b| format!("0x{:02X}", b))
                                        .collect();
                                    println!("  Video format preferences: {}", svrs.join(", "));
                                }
                                if !cea.y420_vics.is_empty() {
                                    let vics: Vec<String> =
                                        cea.y420_vics.iter().map(|v| v.to_string()).collect();
                                    println!("  Y420-only VICs: {}", vics.join(", "));
                                }
                                if !cea.y420_capability_map.is_empty() {
                                    let bytes: Vec<String> = cea
                                        .y420_capability_map
                                        .iter()
                                        .map(|b| format!("0x{:02X}", b))
                                        .collect();
                                    println!("  Y420 cap map: {}", bytes.join(" "));
                                }
                                if let Some(ha) = &cea.hdmi_audio {
                                    println!(
                                        "  HDMI Audio:   max streams {}{}, {} 3D audio descriptor(s){}",
                                        ha.max_streams,
                                        if ha.ms_non_mixed { " (non-mixed)" } else { "" },
                                        ha.audio_3d_descriptors.len(),
                                        ha.speaker_allocation_3d
                                            .map(|a| format!(", 3D layout {:?}", a.channels))
                                            .unwrap_or_default()
                                    );
                                }
                                if !cea.infoframe_descriptors.is_empty() {
                                    println!("  InfoFrames ({}):", cea.infoframe_descriptors.len());
                                    for d in &cea.infoframe_descriptors {
                                        let name = match d.type_code {
                                            infoframe_type::VENDOR_SPECIFIC => "VSI",
                                            infoframe_type::AVI => "AVI",
                                            infoframe_type::SOURCE_PRODUCT_DESCRIPTOR => "SPD",
                                            infoframe_type::AUDIO => "Audio",
                                            infoframe_type::MPEG_SOURCE => "MPEG Source",
                                            infoframe_type::NTSC_VBI => "NTSC VBI",
                                            infoframe_type::DYNAMIC_RANGE_MASTERING => "DRM",
                                            _ => "Unknown",
                                        };
                                        if let Some(oui) = d.vendor_oui {
                                            println!("    {} (OUI=0x{:06X})", name, oui);
                                        } else {
                                            println!("    {}", name);
                                        }
                                    }
                                }
                                if let Some(rc) = &cea.room_configuration {
                                    println!(
                                        "  Room config:  {} speakers, has_locations={}",
                                        rc.speaker_count, rc.has_speaker_locations
                                    );
                                }
                                if !cea.speaker_locations.is_empty() {
                                    println!(
                                        "  Speaker locations ({}):",
                                        cea.speaker_locations.len()
                                    );
                                    for loc in &cea.speaker_locations {
                                        println!(
                                            "    ch={} dist={}",
                                            loc.channel_assignment, loc.distance
                                        );
                                    }
                                }
                                if let Some(dddb) = &cea.vesa_display_device {
                                    println!(
                                        "  VESA DDDB:    iface_type=0x{:X} links={} min_clock={}MHz max_clock={}MHz",
                                        dddb.interface_type,
                                        dddb.num_links,
                                        dddb.min_clock_mhz,
                                        dddb.max_clock_mhz
                                    );
                                    if let (Some(w), Some(h)) =
                                        (dddb.native_width, dddb.native_height)
                                    {
                                        println!("  Native res:   {}x{}", w, h);
                                    }
                                    println!(
                                        "  Color depth:  interface={}bpc display={}bpc",
                                        dddb.interface_color_depth, dddb.display_color_depth
                                    );
                                    if let Some(delay) = dddb.audio_delay_ms {
                                        println!(
                                            "  Audio delay:  {}ms ({})",
                                            delay.abs(),
                                            if delay >= 0 {
                                                "audio late"
                                            } else {
                                                "audio early"
                                            }
                                        );
                                    }
                                }
                                if !cea.vendor_specific_video.is_empty() {
                                    println!(
                                        "  Vendor-Specific Video ({}):",
                                        cea.vendor_specific_video.len()
                                    );
                                    for b in &cea.vendor_specific_video {
                                        println!(
                                            "    OUI=0x{:06X} payload={} byte(s)",
                                            b.oui,
                                            b.payload.len()
                                        );
                                    }
                                }
                                if !cea.vendor_specific_audio.is_empty() {
                                    println!(
                                        "  Vendor-Specific Audio ({}):",
                                        cea.vendor_specific_audio.len()
                                    );
                                    for b in &cea.vendor_specific_audio {
                                        println!(
                                            "    OUI=0x{:06X} payload={} byte(s)",
                                            b.oui,
                                            b.payload.len()
                                        );
                                    }
                                }
                                if !cea.t7_vtdb.is_empty() {
                                    println!("  T7VTDB ({}):", cea.t7_vtdb.len());
                                    for t7 in &cea.t7_vtdb {
                                        println!(
                                            "    {}x{}@{}Hz{}",
                                            t7.mode.width,
                                            t7.mode.height,
                                            rate(t7.mode.refresh_rate),
                                            if t7.y420 { " (Y420)" } else { "" }
                                        );
                                    }
                                }
                                if !cea.t8_vtdb.is_empty() {
                                    let total: usize =
                                        cea.t8_vtdb.iter().map(|t| t.timings.len()).sum();
                                    println!(
                                        "  T8VTDB ({} block(s), {} timing(s)):",
                                        cea.t8_vtdb.len(),
                                        total
                                    );
                                    for t8 in &cea.t8_vtdb {
                                        for mode in &t8.timings {
                                            println!(
                                                "    {}x{}@{}Hz{}{}",
                                                mode.width,
                                                mode.height,
                                                rate(mode.refresh_rate),
                                                if mode.interlaced { "i" } else { "" },
                                                if t8.y420 { " (Y420)" } else { "" }
                                            );
                                        }
                                    }
                                }
                                if !cea.t10_vtdb.is_empty() {
                                    let total: usize =
                                        cea.t10_vtdb.iter().map(|t| t.entries.len()).sum();
                                    println!(
                                        "  T10VTDB ({} block(s), {} entry(ies)):",
                                        cea.t10_vtdb.len(),
                                        total
                                    );
                                    for t10 in &cea.t10_vtdb {
                                        for e in &t10.entries {
                                            println!(
                                                "    {}x{}@{}Hz{}",
                                                e.width,
                                                e.height,
                                                e.refresh_hz,
                                                if e.y420 { " (Y420)" } else { "" }
                                            );
                                        }
                                    }
                                }
                                if let Some(count) = cea.hf_eeodb_extension_count {
                                    println!(
                                        "  HF-EEODB: extension block count override = {}",
                                        count
                                    );
                                }
                                if let Some(ref scdb) = cea.hf_scdb {
                                    println!("  HF-SCDB (HDMI 2.1 sink capabilities):");
                                    if scdb.max_tmds_rate_mhz > 0 {
                                        println!(
                                            "    Max TMDS rate: {} MHz",
                                            scdb.max_tmds_rate_mhz
                                        );
                                    } else {
                                        println!("    Max TMDS rate: ≤340 MHz");
                                    }
                                    println!("    Max FRL rate: {:?}", scdb.max_frl_rate);
                                    let mut flags = Vec::new();
                                    if scdb.scdc_present {
                                        flags.push("SCDC");
                                    }
                                    if scdb.allm {
                                        flags.push("ALLM");
                                    }
                                    if scdb.fva {
                                        flags.push("FVA");
                                    }
                                    if scdb.qms {
                                        flags.push("QMS");
                                    }
                                    if !flags.is_empty() {
                                        println!("    Flags: {}", flags.join(", "));
                                    }
                                    if let (Some(min), Some(max)) =
                                        (scdb.vrr_min_hz, scdb.vrr_max_hz)
                                    {
                                        if max > 0 {
                                            println!("    VRR range: {}–{} Hz", min, max);
                                        }
                                    }
                                    if let Some(ref dsc) = scdb.dsc {
                                        if dsc.dsc_1p2 {
                                            println!(
                                                "    DSC 1.2: max {:?}, {:?}",
                                                dsc.max_frl_rate, dsc.max_slices
                                            );
                                        }
                                    }
                                }
                                if !cea.vtb_ext.is_empty() {
                                    let total: usize =
                                        cea.vtb_ext.iter().map(|v| v.timings.len()).sum();
                                    println!(
                                        "  VTB-EXT ({} block(s), {} timing(s)):",
                                        cea.vtb_ext.len(),
                                        total
                                    );
                                    for (i, vtb) in cea.vtb_ext.iter().enumerate() {
                                        for t in &vtb.timings {
                                            println!(
                                                "    [{}] {}x{}@{}Hz",
                                                i,
                                                t.width,
                                                t.height,
                                                rate(t.refresh_rate)
                                            );
                                        }
                                    }
                                }
                            }

                            if let Some(did) =
                                caps.get_extension_data::<DisplayIdCapabilities>(0x70)
                            {
                                let product_type_str = match did.product_type {
                                    0 => "extension",
                                    1 => "test/measurement",
                                    2 => "desktop/notebook",
                                    3 => "television",
                                    4 => "repeater/translator",
                                    5 => "direct-view LED",
                                    _ => "reserved",
                                };
                                println!(
                                    "  DisplayID:    version=0x{:02X} product_type={} ({})",
                                    did.version, did.product_type, product_type_str
                                );
                            }

                            if let (Some(min_v), Some(max_v)) = (caps.min_v_rate, caps.max_v_rate) {
                                println!("  V-Range:      {} - {} Hz", min_v, max_v);
                            }
                            if let (Some(min_h), Some(max_h)) =
                                (caps.min_h_rate_khz, caps.max_h_rate_khz)
                            {
                                println!("  H-Range:      {} - {} kHz", min_h, max_h);
                            }
                            if let Some(clock) = caps.max_pixel_clock_mhz {
                                println!("  Max Clock:    {} MHz", clock);
                            }

                            // Read back additional typed data stored by CeaDetailsHandler
                            if let Some(cea) = caps.get_extension_data::<CeaDetails>(0xF2) {
                                println!("  CEA Version:   {}", cea.version);
                                println!("  CEA DTD Offset: {}", cea.dtd_offset);
                            }

                            {
                                let active: Vec<_> = caps
                                    .white_points
                                    .iter()
                                    .filter_map(|wp| wp.as_ref())
                                    .collect();
                                if !active.is_empty() {
                                    println!("  White points ({}):", active.len());
                                    for wp in active {
                                        let g = wp.gamma.map(|g: piaf::DisplayGamma| g.value());
                                        println!(
                                            "    [{}] ({:.4},{:.4}) gamma={:?}",
                                            wp.index,
                                            wp.chromaticity.x(),
                                            wp.chromaticity.y(),
                                            g,
                                        );
                                    }
                                }
                            }
                            if !caps.warnings.is_empty() {
                                println!("  Warnings ({}):", caps.warnings.len());
                                for warning in &caps.warnings {
                                    println!("    - {:?}", warning);
                                }
                            }

                            if !caps.supported_modes.is_empty() {
                                println!("  Supported Modes ({}):", caps.supported_modes.len());
                                for mode in caps.supported_modes.iter() {
                                    let interlaced = if mode.interlaced { "i" } else { "" };
                                    let sync = match mode.sync {
                                        Some(piaf::SyncDefinition::DigitalSeparate {
                                            h_sync_positive,
                                            v_sync_positive,
                                        }) => format!(
                                            " {}H{}V",
                                            if h_sync_positive { "+" } else { "-" },
                                            if v_sync_positive { "+" } else { "-" },
                                        ),
                                        Some(_) => " (analog sync)".to_string(),
                                        None => String::new(),
                                    };
                                    let timing =
                                        if mode.h_front_porch != 0 || mode.h_sync_width != 0 {
                                            format!(
                                                " [hfp={} hsw={} vfp={} vsw={}]",
                                                mode.h_front_porch,
                                                mode.h_sync_width,
                                                mode.v_front_porch,
                                                mode.v_sync_width,
                                            )
                                        } else {
                                            String::new()
                                        };
                                    println!(
                                        "    - {}x{}{}@{}Hz{}{}",
                                        mode.width,
                                        mode.height,
                                        interlaced,
                                        rate(mode.refresh_rate),
                                        sync,
                                        timing,
                                    );
                                }
                            }

                            if !parsed.warnings().is_empty() {
                                println!("  Parse warnings: {:?}", parsed.warnings());
                            }
                        }
                        Err(e) => {
                            println!("  Error parsing EDID: {:?}", e);
                        }
                    }
                    found += 1;
                }
            }
        }
    }

    if found == 0 {
        println!("No active displays with EDID data were found.");
    } else {
        println!("\n--- Finished. Found {} display(s). ---", found);
    }
}

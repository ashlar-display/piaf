# CEA-861 Vendor-Specific Data Blocks (outer tag `0x03`)

VSDB blocks share the standard CEA-861 data block envelope: one header byte
(`tag[7:5]=0b011`, `length[4:0]=N`), followed by N payload bytes.  The first
three payload bytes are the IEEE OUI in little-endian order.  Well-known OUIs
decoded by `Cea861Handler`:

| OUI | Owner | Decoded into |
|-----|-------|--------------|
| `0x000C03` (LE: `03 0C 00`) | HDMI Licensing, LLC | `HdmiVsdb` |
| `0xC45DD8` (LE: `D8 5D C4`) | HDMI Forum | `HdmiForumSinkCap` (via `hf_vsdb`) |

Any VSDB whose OUI is not in the table above is silently ignored.

---

## HDMI 1.x VSDB (OUI `0x000C03`)

Source: HDMI 1.4b's vendor-specific data block, as Linux kernel `drm_edid.c` and edid-decode decode it.

```
Byte 1:   Tag (0x03) | Length N
Byte 2:   OUI[0] = 0x03
Byte 3:   OUI[1] = 0x0C
Byte 4:   OUI[2] = 0x00
Byte 5:   Source Physical Address [15:8]  (HDMI CEC A.B nibbles)
Byte 6:   Source Physical Address [7:0]   (HDMI CEC C.D nibbles)
Byte 7:   SUPPORTS_AI[7] | DC_48BIT[6] | DC_36BIT[5] | DC_30BIT[4]
          | DC_Y444[3] | Reserved[2:1] | DVI_DUAL[0]
Byte 8:   Max_TMDS_Clock × 5 MHz  (0 = not reported)
Byte 9:   Latency_Fields_Present[7] | I_Latency_Fields_Present[6] | HDMI_Video_Present[5]
          | Reserved[4:3] | CNC[2:0]
Byte 10:  Video_Latency         (progressive; present when Latency_Fields_Present = 1)
Byte 11:  Audio_Latency         (progressive; present when Latency_Fields_Present = 1)
Byte 12:  Interlaced_Video_Latency  (present when I_Latency_Fields_Present = 1)
Byte 13:  Interlaced_Audio_Latency  (present when I_Latency_Fields_Present = 1)
Bytes 14…N+1: HDMI Video sub-block and 3D extensions (not decoded; ignored)
```

Latency byte decoding: `0` or `255` → unknown; `1–251` → `(byte − 1) × 2` ms;
`252–254` → reserved/unknown.

Decoded into `HdmiVsdb` stored in `Cea861Capabilities::hdmi_vsdb`.

---

## HDMI Forum VSDB / HF-VSDB (OUI `0xC45DD8`)

Source: structure reconstructed from Linux kernel `drm_edid.c`, edid-decode, and
libdisplay-info.

The HF-VSDB carries the **HDMI Forum Sink Capability Data Structure (SCDS)**,
which is identical to the payload of the HF-SCDB (extended tag `0x79`) except
that there are no reserved prefix bytes — the SCDS begins immediately after the
OUI.

```
Byte 1:   Tag (0x03) | Length N          (N ≥ 7 for a valid HF-VSDB)
Byte 2:   OUI[0] = 0xD8
Byte 3:   OUI[1] = 0x5D
Byte 4:   OUI[2] = 0xC4
  -- SCDS (Sink Capability Data Structure) --
Byte 5:   Version (expected 1)
Byte 6:   Max_TMDS_Character_Rate / 5  (MHz; 0 → ≤ 340 MHz)
Byte 7:   SCDC_Present[7] | RR_Capable[6] | Cable_Status[5] | CCBPCI[4]
          | LTE_340Scramble[3] | 3D_Independent_View[2] | 3D_Dual_View[1] | 3D_OSD_Disparity[0]
Byte 8:   Max_FRL_Rate[7:4] | UHD_VIC[3] | DC_48b_420[2] | DC_36b_420[1] | DC_30b_420[0]
  -- optional extended section (bytes 9–11) --
Byte 9:   FAPA_End_Ext[7] | QMS[6] | M_Delta[5] | CinemaVRR[4](deprecated)
          | Neg_MVRR[3] | FVA[2] | ALLM[1] | FAPA_Start[0]
Byte 10:  VRRmax[9:8][7:6] | VRRmin[5:0]
Byte 11:  VRRmax[7:0]
  -- optional DSC section (bytes 12–14) --
Byte 12:  DSC_1p2[7] | DSC_Native_420[6] | QMS_TFRmax[5] | QMS_TFRmin[4]
          | DSC_All_BPC[3] | Reserved[2] | DSC_12bpc[1] | DSC_10bpc[0]
Byte 13:  DSC_Max_FRL_Rate[7:4] | DSC_MaxSlices[3:0]
Byte 14:  Reserved[7:6] | DSC_MaxTotalChunkBytes[5:0]  (1024×(1+field); 0=not reported)
```

`Max_FRL_Rate` and `DSC_Max_FRL_Rate` use the `HdmiForumFrl` enum (values 0–6;
see HF-SCDB documentation in `cea861-extended-tags.md` for the full table).

Decoded into `HdmiForumSinkCap` stored in `Cea861Capabilities::hf_vsdb`.  The
same `HdmiForumSinkCap` type is used for the HF-SCDB (extended tag `0x79`)
stored in `Cea861Capabilities::hf_scdb`.  Modern HDMI 2.1 sinks typically
provide the HF-SCDB; older HDMI 2.0 sinks use the HF-VSDB.

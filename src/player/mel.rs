use std::{ffi::c_void, panic::catch_unwind, slice};

use dolby_vision::rpu::{ConversionMode, dovi_rpu::DoviRpu, rpu_data_nlq::DoviELType};
use memchr::memmem;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(i32)]
enum Failure {
    Fel = 1,
    Invalid = 2,
}

#[derive(Debug)]
pub struct LayerError(i32);

impl LayerError {
    pub(super) fn from_code(code: i32) -> Option<Self> {
        (code != 0).then_some(Self(code))
    }
    pub fn is_fel(&self) -> bool {
        self.0 == Failure::Fel as i32
    }
}
impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            1 => "This Profile 7 picture uses a full enhancement layer (FEL).",
            3 => "MEL compatibility requires a single HEVC video track with both Dolby Vision layers.",
            4 => "The file does not signal a valid HDR10-compatible Profile 7 base layer, enhancement layer and RPU.",
            _ => "RustyFin could not verify Profile 7 metadata for this picture.",
        })
    }
}
impl std::error::Error for LayerError {}

fn convert_rpu(nal: &[u8], mappings: &mut u16) -> Result<Vec<u8>, Failure> {
    let mut rpu = DoviRpu::parse_unspec62_nalu(nal).map_err(|_| Failure::Invalid)?;
    if rpu.dovi_profile != 7
        || rpu.header.bl_video_full_range_flag
        || rpu.header.bl_bit_depth_minus8 != 2
    {
        return Err(Failure::Invalid);
    }
    if rpu.header.use_prev_vdr_rpu_flag {
        let id = rpu.header.prev_vdr_rpu_id;
        if id >= 16 || *mappings & (1 << id) == 0 {
            return Err(Failure::Invalid);
        }
    } else {
        match rpu.el_type {
            Some(DoviELType::MEL) => {
                let id = rpu
                    .rpu_data_mapping
                    .as_ref()
                    .ok_or(Failure::Invalid)?
                    .vdr_rpu_id;
                if id >= 16 {
                    return Err(Failure::Invalid);
                }
                *mappings |= 1 << id;
            }
            Some(DoviELType::FEL) => return Err(Failure::Fel),
            None => return Err(Failure::Invalid),
        }
    }
    rpu.convert_with_mode(ConversionMode::To81)
        .map_err(|_| Failure::Invalid)?;
    rpu.write_hevc_unspec62_nalu().map_err(|_| Failure::Invalid)
}

// hevc_mp4toannexb produces complete access units. The mapping IDs belong to
// the current decode sequence and are cleared by the source on every seek.
fn convert(bytes: &[u8], configuration: bool, mappings: &mut u16) -> Result<Vec<u8>, Failure> {
    let mut prefixes = memmem::find_iter(bytes, &[0, 0, 1]).peekable();
    let first = *prefixes.peek().ok_or(Failure::Invalid)?;
    if bytes[..first].iter().any(|&b| b != 0) {
        return Err(Failure::Invalid);
    }
    let mut output = Vec::with_capacity(bytes.len());
    let mut rpus = 0;
    let mut pictures = 0;
    let mut parameter_sets = 0u8;
    let mut verified = *mappings;
    while let Some(prefix) = prefixes.next() {
        let end = prefixes.peek().map_or(bytes.len(), |&next| {
            next - usize::from(bytes[next - 1] == 0)
        });
        let nal = bytes.get(prefix + 3..end).ok_or(Failure::Invalid)?;
        if nal.len() < 2 || nal[0] & 0x80 != 0 || nal[1] & 7 == 0 {
            return Err(Failure::Invalid);
        }
        let layer = ((nal[0] & 1) << 5) | (nal[1] >> 3);
        let kind = (nal[0] >> 1) & 63;
        if layer != 0 || kind == 63 {
            continue;
        }
        if configuration && !matches!(kind, 32..=34 | 39 | 40) {
            continue;
        }
        output.extend_from_slice(&[0, 0, 0, 1]);
        if kind == 62 {
            rpus += 1;
            if rpus != 1 {
                return Err(Failure::Invalid);
            }
            output.extend_from_slice(&convert_rpu(nal, &mut verified)?);
        } else {
            if kind <= 31 && nal.get(2).ok_or(Failure::Invalid)? & 0x80 != 0 {
                pictures += 1;
            }
            if (32..=34).contains(&kind) {
                parameter_sets |= 1 << (kind - 32);
            }
            output.extend_from_slice(nal);
        }
    }
    if (configuration && parameter_sets != 7) || (!configuration && (pictures != 1 || rpus != 1)) {
        return Err(Failure::Invalid);
    }
    *mappings = verified;
    Ok(output)
}

type Allocate = unsafe extern "C" fn(*mut c_void, i32) -> *mut u8;
pub(super) type Convert =
    unsafe extern "C" fn(*const u8, i32, i32, *mut u16, *mut c_void, Allocate) -> i32;

// Called under the native source's mutex. The packet and mapping mask remain
// live until return. The native allocator supplies exactly the requested space.
pub(super) unsafe extern "C" fn packet(
    data: *const u8,
    len: i32,
    config: i32,
    mappings: *mut u16,
    packet: *mut c_void,
    allocate: Allocate,
) -> i32 {
    if len <= 0 || data.is_null() || mappings.is_null() || packet.is_null() {
        return Failure::Invalid as i32;
    }
    catch_unwind(|| unsafe {
        let bytes = slice::from_raw_parts(data, len as usize);
        let mut verified = *mappings;
        match convert(bytes, config != 0, &mut verified) {
            Ok(output) => {
                let Ok(size) = i32::try_from(output.len()) else {
                    return Failure::Invalid as i32;
                };
                let target = allocate(packet, size);
                if target.is_null() {
                    return -1;
                }
                std::ptr::copy_nonoverlapping(output.as_ptr(), target, output.len());
                *mappings = verified;
                0
            }
            Err(error) => error as i32,
        }
    })
    .unwrap_or(Failure::Invalid as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Upstream dovi_tool RPU vectors, revision d1abe0e27ff2c7ab3339614d06db9f8a058af6b2.
    // The .bin files carry an Annex B prefix in place of the HEVC NAL header.
    const MEL: &[u8] = include_bytes!("testdata/mel_orig.bin");
    const P81: &[u8] = include_bytes!("testdata/mel_to_81.bin");
    const FEL: &[u8] = include_bytes!("testdata/fel_orig.bin");
    const BL: &[u8] = &[0x28, 1, 0xaf, 0, 0, 3, 1, 0x80];
    const EL: &[u8] = &[0x7e, 1, 0x28, 1, 0xaf];
    const HDR_SEI: &[u8] = &[0x4e, 1, 0x89, 0x80];

    fn rpu(fixture: &[u8]) -> Vec<u8> {
        [&[0x7c, 1], &fixture[4..]].concat()
    }
    fn packet(nals: &[&[u8]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for nal in nals {
            bytes.extend_from_slice(&[0, 0, 0, 1]);
            bytes.extend_from_slice(nal);
        }
        bytes
    }

    #[test]
    fn matches_profile_81_reference_and_preserves_base_picture_and_hdr_sei() {
        let input = packet(&[HDR_SEI, BL, EL, &rpu(MEL)]);
        let output = convert(&input, false, &mut 0).unwrap();
        assert_eq!(output, packet(&[HDR_SEI, BL, &rpu(P81)]));
    }

    #[test]
    fn rejects_fel_even_after_mel_and_rejects_ambiguous_pictures() {
        let mut mappings = 0;
        convert(&packet(&[BL, &rpu(MEL)]), false, &mut mappings).unwrap();
        assert_eq!(
            convert(&packet(&[BL, EL, &rpu(FEL)]), false, &mut mappings),
            Err(Failure::Fel)
        );
        let mut bad_crc = rpu(MEL);
        let last = bad_crc.len() - 2;
        bad_crc[last] ^= 1;
        for nals in [
            vec![BL, EL],
            vec![BL, &bad_crc],
            vec![BL, &rpu(P81)],
            vec![BL, &rpu(MEL), &rpu(MEL)],
            vec![EL, &rpu(MEL)],
            vec![BL, BL, &rpu(MEL)],
        ] {
            assert_eq!(
                convert(&packet(&nals), false, &mut mappings),
                Err(Failure::Invalid)
            );
        }
    }

    #[test]
    fn accepts_only_mapping_ids_verified_since_the_last_seek() {
        let mut previous = DoviRpu::parse_unspec62_nalu(&rpu(MEL)).unwrap();
        let id = previous.rpu_data_mapping.take().unwrap().vdr_rpu_id;
        previous.modified = true;
        previous.header.use_prev_vdr_rpu_flag = true;
        previous.header.prev_vdr_rpu_id = id;
        let repeat = packet(&[BL, &previous.write_hevc_unspec62_nalu().unwrap()]);
        assert_eq!(convert(&repeat, false, &mut 0), Err(Failure::Invalid));
        let mut verified = 0;
        convert(&packet(&[BL, &rpu(MEL)]), false, &mut verified).unwrap();
        assert!(convert(&repeat, false, &mut verified).is_ok());
        let mut wrong = 1 << ((id + 1) % 16);
        assert_eq!(convert(&repeat, false, &mut wrong), Err(Failure::Invalid));
    }

    #[test]
    fn strips_enhancement_parameter_sets_and_accepts_three_byte_prefixes() {
        let sets: [&[u8]; 3] = [&[64, 1, 5], &[66, 1, 6], &[68, 1, 7]];
        let mut config = packet(&sets);
        config.extend_from_slice(&[0, 0, 1, 0x7e, 1, 64, 1, 8]);
        assert_eq!(convert(&config[1..], true, &mut 0).unwrap(), packet(&sets));
        assert!(convert(&packet(&sets[..2]), true, &mut 0).is_err());
    }
}

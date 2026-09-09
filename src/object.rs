//! Object file formats.
//!
//! Five of them, all fed from the same regions: Intel HEX, Intel HEX with word
//! addresses, MOS Technology, Motorola S-record, and raw binary.

use crate::cli::ObjFormat;
use crate::image::Region;

/// Cut a region into records of at most `per` data bytes. 8.2: the final
/// record of a region carries the remainder, and `per` came from -o<xx>, which
/// is HEXADECIMAL -- so -o32 means 50 bytes, not 32.
fn records(r: &Region, per: usize) -> Vec<(u32, &[u8])> {
    let per = per.max(1);
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < r.bytes.len() {
        let n = per.min(r.bytes.len() - off);
        out.push((r.start + off as u32, &r.bytes[off..off + n]));
        off += n;
    }
    out
}

fn hex2(v: u32) -> String {
    format!("{:02X}", v & 0xFF)
}
fn hex4(v: u32) -> String {
    format!("{:04X}", v & 0xFFFF)
}

pub fn write(
    format: ObjFormat,
    regions: &[Region],
    per: usize,
    end_addr: u32,
    bug_compatibility: bool,
) -> Vec<u8> {
    match format {
        ObjFormat::Binary => {
            let mut out = Vec::new();
            for r in regions {
                out.extend_from_slice(&r.bytes);
            }
            out
        }
        ObjFormat::IntelHex => intel(regions, per, false, bug_compatibility),
        ObjFormat::IntelWord => intel(regions, per, true, bug_compatibility),
        ObjFormat::MosTech => mos(regions, per),
        ObjFormat::SRecord => srec(regions, per, end_addr),
    }
}

/// 8.3 / 8.7. Intel HEX: `:LLAAAATT<data>CC`, type always 00, checksum the
/// two's complement of the low byte of the sum.
///
/// With `word` (the -g4 variant) the address field carries `pc >> 1`.
///
/// The original computed the checksum from the BYTE address while printing the
/// WORD address (8.7). The two disagree whenever the address is non-zero, so
/// every record after the first fails validation in a conforming HEX loader --
/// the files are simply invalid. The checksum is therefore computed from the
/// address actually printed, and `--bug-compatibility` restores the defect for
/// byte-equality with the original.
fn intel(regions: &[Region], per: usize, word: bool, bug_compatibility: bool) -> Vec<u8> {
    let mut out = String::new();
    for r in regions {
        for (addr, data) in records(r, per) {
            let shown = if word { addr >> 1 } else { addr };
            let summed = if bug_compatibility { addr } else { shown };
            let sum: u32 = data.len() as u32
                + ((summed >> 8) & 0xFF)
                + (summed & 0xFF)
                + data.iter().map(|b| *b as u32).sum::<u32>();
            out.push(':');
            out.push_str(&hex2(data.len() as u32));
            out.push_str(&hex4(shown));
            out.push_str("00");
            for b in data {
                out.push_str(&hex2(*b as u32));
            }
            out.push_str(&hex2(sum.wrapping_neg()));
            out.push('\n');
        }
    }
    out.push_str(":00000001FF\n");
    out.into_bytes()
}

/// 8.4. MOS Technology: `;LLAAAA<data>CCCC`. No type field, and the checksum
/// is a plain 16-bit sum in FOUR hex digits, not complemented.
fn mos(regions: &[Region], per: usize) -> Vec<u8> {
    let mut out = String::new();
    for r in regions {
        for (addr, data) in records(r, per) {
            let sum: u32 = data.len() as u32
                + ((addr >> 8) & 0xFF)
                + (addr & 0xFF)
                + data.iter().map(|b| *b as u32).sum::<u32>();
            out.push(';');
            out.push_str(&hex2(data.len() as u32));
            out.push_str(&hex4(addr));
            for b in data {
                out.push_str(&hex2(*b as u32));
            }
            out.push_str(&hex4(sum));
            out.push('\n');
        }
    }
    out.push_str(";00\n");
    out.into_bytes()
}

/// 8.5. Motorola S-record: `S1LLAAAA<data>CC`, where LL counts the two address
/// bytes and the checksum byte as well as the data -- so a 24-byte record
/// shows 1B.
fn srec(regions: &[Region], per: usize, end_addr: u32) -> Vec<u8> {
    let mut out = String::new();
    for r in regions {
        for (addr, data) in records(r, per) {
            let ll = data.len() as u32 + 3;
            let sum: u32 = ll
                + ((addr >> 8) & 0xFF)
                + (addr & 0xFF)
                + data.iter().map(|b| *b as u32).sum::<u32>();
            out.push_str("S1");
            out.push_str(&hex2(ll));
            out.push_str(&hex4(addr));
            for b in data {
                out.push_str(&hex2(*b as u32));
            }
            out.push_str(&hex2(!sum));
            out.push('\n');
        }
    }
    // The S9 terminator carries the .END address. 8.5: that operand is
    // range-checked and masked to 16 bits, so the four-digit field cannot
    // overflow -- the original could emit an eleven-character S9 record whose
    // length disagreed with its contents, and the reference output was updated
    // rather than preserving it.
    let a = end_addr & 0xFFFF;
    let sum = 3 + ((a >> 8) & 0xFF) + (a & 0xFF);
    out.push_str("S903");
    out.push_str(&hex4(a));
    out.push_str(&hex2(!sum));
    out.push('\n');
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The last record of tests/golden/51-g0.obj and friends: the same six
    /// bytes at 0x0168 in each of the four text formats.
    fn tail() -> Region {
        Region {
            start: 0x0168,
            bytes: vec![0x12, 0x62, 0x12, 0x63, 0x12, 0x56],
        }
    }

    fn text(v: Vec<u8>) -> String {
        String::from_utf8(v).unwrap()
    }

    #[test]
    fn intel_hex_matches_the_corpus() {
        let s = text(intel(&[tail()], 0x18, false, false));
        assert_eq!(s, ":0601680012621263125640\n:00000001FF\n");
    }

    #[test]
    fn mos_technology_matches_the_corpus() {
        let s = text(mos(&[tail()], 0x18));
        assert_eq!(s, ";06016812621263125601C0\n;00\n");
    }

    #[test]
    fn srecord_matches_the_corpus() {
        let s = text(srec(&[tail()], 0x18, 0));
        assert_eq!(s, "S10901681262126312563C\nS9030000FC\n");
    }

    #[test]
    fn a_full_length_record_shows_the_srecord_length_convention() {
        // 24 data bytes print as 1B, counting the address and checksum bytes.
        let r = Region {
            start: 0,
            bytes: (0..24).map(|_| 0u8).collect(),
        };
        let s = text(srec(&[r], 0x18, 0));
        assert!(s.starts_with("S11B0000"), "got {}", &s[..12]);
    }

    #[test]
    fn word_addressing_checksum_is_correct_by_default_and_defective_under_the_flag() {
        // 8.7: the original printed the WORD address but computed the checksum
        // from the BYTE address, so -g0 and -g4 emitted the same checksum for
        // the same data at different printed addresses -- invalid records.
        // Default now sums the printed address; --bug-compatibility restores
        // the defect, which is what the corpus pins.
        let data: Vec<u8> = vec![
            0x36, 0x37, 0x34, 0x56, 0x35, 0x12, 0x01, 0x93, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D,
            0x5E, 0x5F, 0x56, 0x57, 0x54, 0x56, 0x55, 0x12, 0xB0, 0x81,
        ];
        let r = Region {
            start: 0x18,
            bytes: data,
        };
        let cs = |s: &str| {
            let l = s.lines().next().unwrap();
            l[l.len() - 2..].to_string()
        };
        let g0 = text(intel(std::slice::from_ref(&r), 0x18, false, false));
        let bug = text(intel(std::slice::from_ref(&r), 0x18, true, true));
        let fixed = text(intel(&[r], 0x18, true, false));

        assert!(g0.starts_with(":18001800"), "{}", &g0[..12]);
        assert!(bug.starts_with(":18000C00"), "{}", &bug[..12]);
        assert!(fixed.starts_with(":18000C00"), "{}", &fixed[..12]);

        // The defect: same checksum as -g0 despite a different printed address.
        assert_eq!(cs(&bug), cs(&g0));
        assert_eq!(cs(&bug), "33");
        // Corrected: the checksum matches the address actually printed, which
        // is 0x0C higher, so the sum is 0x0C lower.
        assert_eq!(cs(&fixed), "3F");

        // And a corrected record actually validates: the whole record,
        // checksum included, sums to zero in the low byte.
        let line = fixed.lines().next().unwrap();
        let total: u32 = (1..line.len())
            .step_by(2)
            .map(|i| u32::from_str_radix(&line[i..i + 2], 16).unwrap())
            .sum();
        assert_eq!(total & 0xFF, 0, "a conforming loader must accept it");
    }

    #[test]
    fn record_length_is_hexadecimal() {
        // 8.2: -o32 means 0x32 = 50 bytes per record, not 32.
        let r = Region {
            start: 0,
            bytes: (0..60u8).collect(),
        };
        let recs = records(&r, 0x32);
        assert_eq!(recs[0].1.len(), 50);
        assert_eq!(recs[1].1.len(), 10);
        let recs = records(&r, 0x08);
        assert_eq!(recs[0].1.len(), 8);
    }

    #[test]
    fn binary_has_no_framing() {
        let out = write(ObjFormat::Binary, &[tail()], 0x18, 0, false);
        assert_eq!(out, vec![0x12, 0x62, 0x12, 0x63, 0x12, 0x56]);
    }

    #[test]
    fn an_end_address_reaches_the_s9_record() {
        let s = text(srec(&[tail()], 0x18, 0x1234));
        assert!(s.ends_with("S9031234B6\n"), "got {:?}", s.lines().last());
    }
}

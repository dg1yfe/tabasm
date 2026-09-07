//! Listing layout (9.1-9.4).
//!
//! The line is a fixed 24-character prefix followed by the source text
//! exactly as written:
//!
//! ```text
//! 0339   0200 18 1F 34 12         BRCLR   addr1,Y,bmsk,lab1
//! ^^^^   ^^^^ ^^^^^^^^^^^^        ^ source from column 24 (0-based)
//! |  |   |    bytes, four "XX " fields padded to 12
//! |  |   program counter, 4 hex
//! |  include depth marker
//! line number
//! ```

/// 9.1: at most four bytes per line. The 3.2 release notes and an in-source
/// comment both claim six; both are stale, and the observed output is
/// authoritative.
pub const BYTES_PER_LINE: usize = 4;

/// 9.2: the include-depth marker saturates at three '+'.
fn depth_marker(depth: usize) -> &'static str {
    match depth {
        0 | 1 => "   ",
        2 => "+  ",
        3 => "++ ",
        _ => "+++",
    }
}

fn bytes_field(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes.iter().take(BYTES_PER_LINE) {
        s.push_str(&format!("{:02X} ", b));
    }
    s
}

/// The first line for a source line: prefix padded to 24 columns, then the
/// source. A line that generates no bytes and has no source text is therefore
/// exactly 24 characters, trailing spaces included.
///
/// `skipped` sets the marker in column 11 for a line inside a false
/// conditional branch (9.3); such lines are still listed.
pub fn line(lineno: u32, depth: usize, pc: u32, skipped: bool, bytes: &[u8], source: &str) -> String {
    // 9.1: the generated prefix is upper-cased BEFORE the source is appended,
    // so a lower-case mnemonic keeps its case beside upper-case hex.
    let prefix = format!(
        "{:04}{}{:04X}{}{:<12}",
        lineno,
        depth_marker(depth),
        pc & 0xFFFF,
        if skipped { '~' } else { ' ' },
        bytes_field(bytes)
    )
    .to_uppercase();
    format!("{}{}", prefix, source)
}

/// A continuation line for an instruction or directive that generated more
/// than four bytes. 9.1: it repeats the source line number, shows the
/// advancing address, and leaves the source column empty -- and, unlike the
/// first line, its byte field is NOT padded out to 12 columns.
pub fn continuation(lineno: u32, depth: usize, pc: u32, bytes: &[u8]) -> String {
    format!(
        "{:04}{}{:04X} {}",
        lineno,
        depth_marker(depth),
        pc & 0xFFFF,
        bytes_field(bytes)
    )
    .to_uppercase()
}

/// A line with .NOCODES in force: 4.10 suppresses the address and byte
/// columns entirely, leaving only the source text.
pub fn line_nocodes(source: &str) -> String {
    source.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_match_the_corpus_byte_for_byte() {
        // tests/golden/51.lst line 24.
        assert_eq!(
            line(24, 0, 0x0002, false, &[0x28], "        ADD  A,R0     ;28    1   NOP 1"),
            "0024   0002 28                  ADD  A,R0     ;28    1   NOP 1"
        );
        // A line with no bytes and no source is exactly the 24-char prefix.
        let blank = line(4, 0, 0, false, &[], "");
        assert_eq!(blank, "0004   0000             ");
        assert_eq!(blank.len(), 24);
    }

    #[test]
    fn a_five_byte_instruction_wraps_after_four() {
        // tests/golden/68.lst lines 339-340: BRCLR emits 18 1F 34 12 EB.
        assert_eq!(
            line(339, 0, 0x0200, false, &[0x18, 0x1F, 0x34, 0x12], "        BRCLR   addr1,Y,bmsk,lab1"),
            "0339   0200 18 1F 34 12         BRCLR   addr1,Y,bmsk,lab1"
        );
        let cont = continuation(339, 0, 0x0204, &[0xEB]);
        assert_eq!(cont, "0339   0204 EB ");
        assert_eq!(cont.len(), 15, "a continuation is not padded to 24");
    }

    #[test]
    fn source_case_survives_the_upper_casing_of_the_prefix() {
        // tests/golden/80.lst line 19.
        assert_eq!(
            line(19, 0, 0, false, &[], "n:          equ 20h"),
            "0019   0000             n:          equ 20h"
        );
    }

    #[test]
    fn skipped_lines_carry_a_tilde_in_column_11() {
        // tests/golden/undef.lst lines 33-34.
        assert_eq!(
            line(33, 0, 2, true, &[], "#ifdef FOO"),
            "0033   0002~            #ifdef FOO"
        );
    }

    #[test]
    fn include_depth_saturates_at_three_plus_signs() {
        assert_eq!(depth_marker(0), "   ");
        assert_eq!(depth_marker(1), "   ");
        assert_eq!(depth_marker(2), "+  ");
        assert_eq!(depth_marker(3), "++ ");
        assert_eq!(depth_marker(9), "+++");
    }
}

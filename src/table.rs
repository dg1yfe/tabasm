//! The `.tab` instruction-table format.

use crate::limits::*;

/// An encoding rule, keyed on the FIRST TWO CHARACTERS of the RULE column
/// (5.4). This is why `NOP` and `NOTOUCH` are the same rule, and why a literal
/// `COMBREL` would select `CO` rather than `CR`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rule(pub u16);

impl Rule {
    pub fn from_str(s: &str) -> Rule {
        let mut it = s.bytes();
        let a = it.next().unwrap_or(b' ').to_ascii_uppercase() as u16;
        let b = it.next().unwrap_or(b' ').to_ascii_uppercase() as u16;
        Rule((a << 8) | b)
    }
    pub const fn key(a: u8, b: u8) -> Rule {
        Rule(((a as u16) << 8) | b as u16)
    }
}

// Phase A: the fourteen rules the 8-bit tables use (7.3-7.16).
pub const NO: Rule = Rule::key(b'N', b'O');
pub const JM: Rule = Rule::key(b'J', b'M');
pub const JT: Rule = Rule::key(b'J', b'T');
pub const R1: Rule = Rule::key(b'R', b'1');
pub const ZP: Rule = Rule::key(b'Z', b'P');
pub const MZ: Rule = Rule::key(b'M', b'Z');
pub const MB: Rule = Rule::key(b'M', b'B');
pub const ZB: Rule = Rule::key(b'Z', b'B');
pub const ZI: Rule = Rule::key(b'Z', b'I');
pub const CO: Rule = Rule::key(b'C', b'O');
pub const CR: Rule = Rule::key(b'C', b'R');
pub const CS: Rule = Rule::key(b'C', b'S');
pub const SW: Rule = Rule::key(b'S', b'W');
pub const R3REL: Rule = Rule::key(b'3', b'R');

// Phase B: TMS320, TMS7000, 8096 (7.22-7.24).
pub const T1: Rule = Rule::key(b'T', b'1');
pub const TD: Rule = Rule::key(b'T', b'D');
pub const TL: Rule = Rule::key(b'T', b'L');
pub const T5: Rule = Rule::key(b'T', b'5');
pub const TA: Rule = Rule::key(b'T', b'A');
pub const SU: Rule = Rule::key(b'S', b'U');
pub const R2: Rule = Rule::key(b'R', b'2');
pub const I1: Rule = Rule::key(b'I', b'1');
pub const I2: Rule = Rule::key(b'I', b'2');
pub const I3: Rule = Rule::key(b'I', b'3');
pub const I4: Rule = Rule::key(b'I', b'4');
pub const I5: Rule = Rule::key(b'I', b'5');
pub const I6: Rule = Rule::key(b'I', b'6');
pub const I7: Rule = Rule::key(b'I', b'7');
pub const I8: Rule = Rule::key(b'I', b'8');

// 7.25: implemented but selected by no shipped table, so nothing in the corpus
// constrains them. Specified from behaviour alone; treat as unverified.
pub const T2: Rule = Rule::key(b'T', b'2');
pub const T3: Rule = Rule::key(b'T', b'3');
pub const T4: Rule = Rule::key(b'T', b'4');
pub const T6: Rule = Rule::key(b'T', b'6');
pub const A3: Rule = Rule::key(b'3', b'A');
pub const CN: Rule = Rule::key(b'C', b'N');
pub const C5: Rule = Rule::key(b'C', b'5');
pub const SZ: Rule = Rule::key(b'S', b'Z');
pub const SB: Rule = Rule::key(b'S', b'B');
pub const R3: Rule = Rule::key(b'R', b'3');
pub const ZW: Rule = Rule::key(b'Z', b'W');
pub const ZD: Rule = Rule::key(b'Z', b'D');
pub const ZL: Rule = Rule::key(b'Z', b'L');

#[derive(Clone, Debug)]
pub struct Row {
    pub mnemonic: String,
    pub args: String,
    pub opcode: u32,
    pub opcode_bytes: u8,
    pub arg_bytes: u8,
    pub rule: Rule,
    pub class: u32,
    pub shift: u32,
    pub or: u32,
    /// The post-rule transform `argval = (argval << post_shift) | post_or`.
    ///
    /// v1 states this once per table -- the absence of `.NOARGSHIFT` -- which is
    /// why the same `OR` column is a value in tasm80.tab and a mask in
    /// tasm3210.tab. Holding it per row removes that ambiguity and is what the
    /// v2 format writes directly. For a v1 table both fields are `shift`/`or`
    /// when the step applies and zero when it does not; `(x << 0) | 0` is the
    /// identity, so either case reproduces exactly.
    pub post_shift: u32,
    pub post_or: u32,
    /// Set when NBYTES was smaller than the opcode, which 5.4 requires be
    /// diagnosed and the argument count forced to zero.
    pub short_count: bool,
}

#[derive(Clone, Debug)]
pub struct RegSet {
    pub name: String,
    pub mask: u32,
    pub class: u32,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub selector: String,
    pub banner: String,
    pub rows: Vec<Row>,
    pub regsets: Vec<RegSet>,
    pub msfirst: bool,    // opcode byte order (distinct from the SOURCE .MSFIRST)
    pub wordaddrs: bool,  // one address unit is two bytes
    pub noargshift: bool, // v1 metadata; the step itself now lives on the Row
    pub wildcard: char,
    /// The character an `ARGS` pattern uses for a register-set slot. v1 spells
    /// it `!`; v2 uses a sentinel that cannot occur in source operand text.
    pub regmark: u8,
    /// How many auxiliary registers the target has, for 7.20's `arp_val`.
    ///
    /// v1 cannot state this: the original decides it by string-comparing the
    /// `-<nn>` selector against "3225", the one place behaviour depends on
    /// which table was NAMED. Derived from the selector when loading v1, read
    /// from `%aux-registers` in v2.
    pub aux_registers: u32,
}

pub enum LoadError {
    Open(String),
    TooManyRows,
    TooManyRegsets,
    /// A v2 table that does not parse: line number and what was wrong.
    Syntax(usize, String),
}

/// All-hex-digits, and not empty. Used to tell a real SHIFT/OR column from
/// trailing commentary: thirteen rows in tasm48.tab carry `;8041` in field 7.
fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex_val(s: &str) -> u32 {
    let mut v: u32 = 0;
    for c in s.chars() {
        match c.to_digit(16) {
            Some(d) => v = v.saturating_mul(16).saturating_add(d),
            None => break,
        }
    }
    v
}

fn dec_val(s: &str) -> u32 {
    let mut v: u32 = 0;
    for c in s.chars() {
        match c.to_digit(10) {
            Some(d) => v = v.saturating_mul(10).saturating_add(d),
            None => break,
        }
    }
    v
}

impl Table {
    pub fn new(selector: &str) -> Table {
        Table {
            selector: selector.to_string(),
            banner: String::new(),
            rows: Vec::new(),
            regsets: Vec::new(),
            msfirst: false,
            wordaddrs: false,
            noargshift: false,
            wildcard: '*',
            regmark: b'!',
            // 7.20: three bits for the C25, one for everything else.
            aux_registers: if selector == "3225" { 8 } else { 2 },
        }
    }

    /// Load the first of `paths` that exists.
    ///
    /// The format is decided by the file's *content*, not its name: a table whose
    /// first substantive line is `%format` parses as v2 whatever it is called.
    pub fn load_first(paths: &[String], selector: &str) -> Result<Table, LoadError> {
        for path in paths {
            if let Ok(bytes) = std::fs::read(path) {
                // Tables are data from outside; do not assume valid UTF-8.
                let text = String::from_utf8_lossy(&bytes).into_owned();
                return if is_v2(&text) {
                    crate::table2::parse(&text, selector)
                } else {
                    Self::load_v1(&text, selector)
                };
            }
        }
        Err(LoadError::Open(paths.first().cloned().unwrap_or_default()))
    }

    /// Convenience for a single known path, used by the tests.
    pub fn load(path: &str, selector: &str) -> Result<Table, LoadError> {
        Self::load_first(&[path.to_string()], selector)
    }

    /// True when the first line that is neither blank nor a `;` comment opens
    /// with `%format`.

    pub(crate) fn load_v1(text: &str, selector: &str) -> Result<Table, LoadError> {
        let mut t = Table::new(selector);

        for (n, raw) in text.lines().enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            // 5.1: dispatch on the FIRST CHARACTER ONLY. There is no comment
            // syntax; anything that is not a banner, a directive, or a row
            // starting in column 1 with an upper-case letter is skipped.
            match line.as_bytes().first() {
                Some(b'"') if n == 0 => t.banner = banner_text(line),
                Some(b'.') => t.directive(line),
                Some(c) if c.is_ascii_uppercase() => {
                    if t.rows.len() >= MAX_TABLE_ROWS {
                        return Err(LoadError::TooManyRows);
                    }
                    if let Some(row) = parse_row(line) {
                        t.rows.push(row);
                    }
                }
                _ => {}
            }
        }
        if t.regsets.len() > MAX_REGSETS {
            return Err(LoadError::TooManyRegsets);
        }
        // Resolve the table-level default step onto the rows (see Row::post_*).
        if !t.noargshift {
            for r in &mut t.rows {
                r.post_shift = r.shift;
                r.post_or = r.or;
            }
        }
        Ok(t)
    }

    fn directive(&mut self, line: &str) {
        let mut it = line.split_whitespace();
        let name = it.next().unwrap_or("");
        let upper = name.to_ascii_uppercase();
        match upper.as_str() {
            ".MSFIRST" => self.msfirst = true,
            ".LSFIRST" => self.msfirst = false,
            ".WORDADDRS" => self.wordaddrs = true,
            ".NOARGSHIFT" => self.noargshift = true,
            ".REGSET" => {
                let n = it.next().unwrap_or("");
                let m = it.next().unwrap_or("");
                let c = it.next().unwrap_or("");
                if !n.is_empty() && self.regsets.len() < MAX_REGSETS {
                    self.regsets.push(RegSet {
                        name: n.to_string(),
                        mask: hex_val(m),
                        class: if c.is_empty() { 1 } else { hex_val(c) },
                    });
                }
            }
            _ => {
                // 5.3: `.ALTWILD[c]` -- the wildcard becomes the character
                // immediately following the directive, or '@' if none. The
                // TMS320 tables need this because '*' is their indirect
                // addressing syntax; tasm70.tab declares `.ALTWILD+`.
                if upper.starts_with(".ALTWILD") {
                    let tail = &name[".ALTWILD".len().min(name.len())..];
                    self.wildcard = match tail.chars().next() {
                        Some(c) if (c as u32) >= 33 && (c as u32) <= 127 => c,
                        _ => '@',
                    };
                }
            }
        }
    }
}

fn banner_text(line: &str) -> String {
    // 5.2: the first quote opens, the next closes; only the text between is
    // used. Trailing spaces inside the quotes are deliberate padding.
    let mut parts = line.splitn(3, '"');
    parts.next();
    parts.next().unwrap_or("").to_string()
}

/// Parse one instruction row. Shared with `.ADDINSTR`, which takes the same
/// syntax from a source file (4.11) -- so this is reachable from hostile input
/// and must not panic or allocate unboundedly.
pub fn is_v2(text: &str) -> bool {
    for raw in text.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with(';') {
            continue;
        }
        return l.starts_with("%format");
    }
    false
}

pub fn parse_row(line: &str) -> Option<Row> {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() < 6 {
        return None;
    }

    // 5.4: opcode byte count is the hex digit count divided by two, and is
    // never stated in the table. Bound it: security.sh feeds 200-digit opcodes.
    let opcode_digits = f[2].bytes().take_while(|b| b.is_ascii_hexdigit()).count();
    let opcode_bytes = (opcode_digits / 2).clamp(1, 4) as u8;
    let opcode = hex_val(f[2]);

    let nbytes = dec_val(f[3]).min(255) as u8;

    // 5.4: argument bytes are DERIVED as NBYTES - opcode_bytes, and the
    // subtraction must be guarded. Unguarded it underflowed a ubyte to ~250 in
    // the original and drove a runaway emission loop.
    let (arg_bytes, short_count) = match nbytes.checked_sub(opcode_bytes) {
        Some(n) => (n, false),
        None => (0, true),
    };

    // SHIFT and OR are optional and positional, and both are hex. Trailing
    // commentary is not: tasm48.tab has thirteen rows whose seventh field is
    // `;8041`. Take field 7 only when it really is hex, and field 8 only after.
    let shift = f.get(6).filter(|s| is_hex(s)).map(|s| hex_val(s)).unwrap_or(0);
    let or = if f.get(6).map_or(false, |s| is_hex(s)) {
        f.get(7).filter(|s| is_hex(s)).map(|s| hex_val(s)).unwrap_or(0)
    } else {
        0
    };

    Some(Row {
        mnemonic: f[0].to_ascii_uppercase(),
        args: f[1].to_string(),
        opcode,
        opcode_bytes,
        arg_bytes,
        rule: Rule::from_str(f[4]),
        class: hex_val(f[5]),
        shift,
        or,
        // Filled in by the caller once the table's directives are known; a row
        // added by `.ADDINSTR` inherits nothing and so takes the identity.
        post_shift: 0,
        post_or: 0,
        short_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The legacy tables are not shipped, so these fixtures are written inline.
    /// That also makes each test say exactly which property it is about, rather
    /// than depending on a particular processor's file.
    fn load(text: &str, selector: &str) -> Table {
        Table::load_v1(text, selector).ok().unwrap()
    }

    const HEAD: &str = "\"TASM 8051 Assembler.    \"\n";

    #[test]
    fn a_row_is_a_line_starting_with_an_upper_case_letter() {
        // There is no comment syntax: `/*` and `;` lines are skipped because
        // they begin with neither an upper-case letter nor a '.', and an
        // indented or lower-case row is skipped for the same reason.
        let t = load(
            &format!(
                "{}/* a comment, by accident of the first character\n\
                 ; so is this\n\
                 NOP \"\" 00 1 NOP 1\n\
                 \x20 ADD A,R0 28 1 NOP 1\n\
                 add A,R1 29 1 NOP 1\n",
                HEAD
            ),
            "51",
        );
        assert_eq!(t.rows.len(), 1, "only the column-1 upper-case row is a row");
        assert_eq!(t.rows[0].mnemonic, "NOP");
    }

    #[test]
    fn slash_star_in_an_operand_pattern_is_data() {
        // `/` is the 8051 complement-bit operator and `*` the wildcard. Treating
        // `/*` as a comment leader would delete this row.
        let t = load(&format!("{}ANL  C,/*     b0 2 NOP 1\n", HEAD), "51");
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0].args, "C,/*");
        assert_eq!(t.rows[0].opcode, 0xb0);
    }

    #[test]
    fn the_banner_keeps_the_padding_inside_its_quotes() {
        assert_eq!(load(HEAD, "51").banner, "TASM 8051 Assembler.    ");
        // No quote on line 1 means no banner, rather than a mis-read one.
        assert_eq!(load("no quote here\n", "51").banner, "");
    }

    #[test]
    fn opcode_width_comes_from_the_digit_count_and_arg_bytes_are_derived() {
        let t = load(
            &format!(
                "{}ACALL *       11 2 JMP 1 0 F800\n\
                 LCALL *       12 3 SWAP 1\n\
                 BIT   *,(IX*) CBDD 4 ZBIT 1 0 4600\n",
                HEAD
            ),
            "51",
        );
        let (acall, lcall, bit) = (&t.rows[0], &t.rows[1], &t.rows[2]);
        assert_eq!((acall.opcode, acall.opcode_bytes, acall.arg_bytes), (0x11, 1, 1));
        assert_eq!(acall.rule, JM);
        assert_eq!((acall.shift, acall.or), (0, 0xF800));
        assert_eq!((lcall.opcode_bytes, lcall.arg_bytes, lcall.rule), (1, 2, SW));
        // Four hex digits is two opcode bytes, so 4 - 2 = 2 argument bytes.
        assert_eq!((bit.opcode_bytes, bit.arg_bytes), (2, 2));
    }

    #[test]
    fn the_default_step_is_resolved_onto_each_row() {
        // Without .NOARGSHIFT the table-level transform applies, so the row
        // carries it; with it, the columns belong to the rule alone.
        let with = load(&format!("{}BIT *,(IX*) CBDD 4 ZBIT 1 0 4600\n", HEAD), "80");
        assert_eq!((with.rows[0].post_shift, with.rows[0].post_or), (0, 0x4600));
        let without = load(
            &format!("{}.NOARGSHIFT\nADD !,@,@ 0088 2 T1 1 8 0F00\n", HEAD),
            "3225",
        );
        assert_eq!((without.rows[0].post_shift, without.rows[0].post_or), (0, 0));
        assert_eq!((without.rows[0].shift, without.rows[0].or), (8, 0x0F00));
    }

    #[test]
    fn trailing_commentary_is_not_read_as_shift_or_or() {
        // A seventh field is SHIFT only if it parses as hexadecimal; several
        // legacy rows carry `;8041` there instead.
        let t = load(&format!("{}EN   DMA    E5 1 NOP 2  ;8041\n", HEAD), "48");
        assert_eq!((t.rows[0].class, t.rows[0].shift, t.rows[0].or), (2, 0, 0));
    }

    #[test]
    fn directives_are_read() {
        let t = load(
            &format!(
                "{}.MSFIRST\n.ALTWILD\n.WORDADDRS\n.NOARGSHIFT\n\
                 .REGSET *BR0+   F0 1\n.REGSET *0+     E0 1\n",
                HEAD
            ),
            "3225",
        );
        assert!(t.msfirst && t.wordaddrs && t.noargshift);
        assert_eq!(t.wildcard, '@', "a bare .ALTWILD means '@'");
        assert_eq!(t.regsets.len(), 2);
        assert_eq!(t.regsets[0].name, "*BR0+");
        // The auxiliary-register width is not in the file; it comes from the
        // selector, which is why the current format states it outright.
        assert_eq!(t.aux_registers, 8);
        assert_eq!(load(HEAD, "51").aux_registers, 2);
        assert_eq!(load(&format!("{}.ALTWILD+\n", HEAD), "70").wildcard, '+');
    }

    #[test]
    fn the_no_operand_spelling_is_two_quotes() {
        // The alias rows that once lacked this column could never match.
        let t = load(
            &format!("{}IM0  \"\"      46ED 2 NOP 1\nIM   0       46ED 2 NOP 1\n", HEAD),
            "80",
        );
        assert_eq!(t.rows[0].args, "\"\"");
        assert_eq!((t.rows[0].opcode, t.rows[0].opcode_bytes, t.rows[0].rule), (0x46ED, 2, NO));
        assert_eq!(t.rows[1].args, "0");
    }

    #[test]
    fn a_byte_count_under_the_opcode_size_is_flagged_not_wrapped() {
        // "FF" is not decimal, so the count reads 0 and the subtraction would
        // underflow a byte to ~255.
        let r = parse_row("FOO *,* 12 FF ZW 1").unwrap();
        assert!(r.short_count);
        assert_eq!(r.arg_bytes, 0);
    }

    #[test]
    fn malformed_rows_are_skipped_rather_than_crashing() {
        assert!(parse_row("NOP").is_none());
        assert!(parse_row("").is_none());
        let long = format!("NOP \"\" {} 1 NOP 1", "A".repeat(200));
        let r = parse_row(&long).unwrap();
        assert!(r.opcode_bytes <= 4, "a 200-digit opcode must be bounded");
    }

    #[test]
    fn rule_names_are_keyed_on_two_characters() {
        // Which is why COMBREL selects the combine rule, not CR.
        assert_eq!(Rule::from_str("NOP"), Rule::from_str("NOTOUCH"));
        assert_eq!(Rule::from_str("COMB"), Rule::from_str("COMBINE"));
        assert_eq!(Rule::from_str("COMBREL"), CO);
        assert_eq!(Rule::from_str("CREL"), CR);
    }
}

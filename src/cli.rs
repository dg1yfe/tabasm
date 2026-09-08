//! Command line, environment and file naming.

use crate::limits::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjFormat {
    IntelHex,  // -g0, the default
    MosTech,   // -g1, -m
    SRecord,   // -g2
    Binary,    // -g3, -b
    IntelWord, // -g4
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LabelTable {
    None,
    Short, // -l
    Long,  // -ll
    All,   // -la
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Table selector text, e.g. "51" or "3225". Kept as the literal string
    /// because 7.20's arp_val compares it against "3225" -- the one place the
    /// assembler's behaviour depends on which table was *named* rather than on
    /// the table's contents.
    pub table: Option<String>,
    pub strict: u32,       // -a<xx>, hex bit mask (1.4)
    pub format: ObjFormat, // -g<x>
    pub block: bool,       // -c, contiguous block output
    pub defines: Vec<String>, // -d<macro>
    pub expand: bool,      // -e, list macro-expanded source
    pub fill: Option<u8>,  // -f<xx>, pre-fill the image
    pub hex_dump: bool,    // -h
    pub ignore_case: bool, // -i
    pub labels: LabelTable, // -l, -ll, -la
    pub bytes_per_record: u32, // -o<xx>, HEX (8.2)
    pub page_lines: Option<u32>, // -p<lines>
    pub quiet: bool,       // -q, suppress the listing
    pub symfile: bool,     // -s
    pub class_mask: u32,   // -x<xx>, hex, default 1 (5.4 CLASS)
    pub timing: bool,      // -y, lower case only (1.3)
    pub debug: bool,       // -z, trace to stderr
    pub compatibility: bool,      // --compatibility (1.3, 3.1, 4.7)
    pub report_compatibility: bool, // --report-compatibility (see main::prog)
    /// --broken-sort-compatibility: reproduce the original's early-stopping
    /// shaker sort for the symbol order instead of sorting properly.
    pub broken_sort: bool,
    pub files: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            table: None,
            // 1.4: the mask is NOT all opt-in. Its default is 0x06, so the
            // unused-argument-bytes and duplicate-label checks run without -a.
            strict: 0x06,
            format: ObjFormat::IntelHex,
            block: false,
            defines: Vec::new(),
            expand: false,
            fill: None,
            hex_dump: false,
            ignore_case: false,
            labels: LabelTable::None,
            bytes_per_record: 0x18, // 24 (8.2)
            page_lines: None,
            quiet: false,
            symfile: false,
            class_mask: 1, // 1.3: -x default is 1
            timing: false,
            debug: false,
            compatibility: false,
            report_compatibility: false,
            broken_sort: false,
            files: Vec::new(),
        }
    }
}

/// Parse a leading run of hex digits, returning 0 for an empty field. Values
/// are bounded rather than allowed to overflow: a 200-digit opcode field is one
/// of the hostile inputs the robustness suite feeds us.
fn hex(s: &str) -> u32 {
    let mut v: u32 = 0;
    for c in s.chars() {
        match c.to_digit(16) {
            Some(d) => v = v.saturating_mul(16).saturating_add(d),
            None => break,
        }
    }
    v
}

fn dec(s: &str) -> u32 {
    let mut v: u32 = 0;
    for c in s.chars() {
        match c.to_digit(10) {
            Some(d) => v = v.saturating_mul(10).saturating_add(d),
            None => break,
        }
    }
    v
}

/// Warnings raised while parsing, emitted by the caller once the message
/// prefix is known.
pub struct Parsed {
    pub opts: Options,
    pub warnings: Vec<String>,
}

impl Options {
    /// 1.1: options and file names may be interleaved. An argument beginning
    /// with '-' is an option; anything else is the next positional file name.
    /// 1.7: TASMOPTS is appended to the command line.
    pub fn parse(argv: &[String], tasmopts: Option<&str>) -> Parsed {
        let mut o = Options::default();
        let mut warnings = Vec::new();

        let mut args: Vec<String> = argv.to_vec();
        if let Some(extra) = tasmopts {
            args.extend(extra.split_whitespace().map(|s| s.to_string()));
        }

        for arg in &args {
            if let Some(body) = arg.strip_prefix('-') {
                o.option(body, &mut warnings);
            } else if o.files.len() < MAX_FILE_ARGS {
                o.files.push(arg.clone());
            } else {
                // 10.3: non-fatal, the extra name is ignored.
                warnings.push(format!("too many file names (max {}): {}", MAX_FILE_ARGS, arg));
            }
        }
        Parsed { opts: o, warnings }
    }

    fn option(&mut self, body: &str, warnings: &mut Vec<String>) {
        // The two long options. --report-compatibility is ours, not the
        // original's: it keeps the "tasm:" message prefix while the binary is
        // named tabasm, so the golden corpus stays byte-comparable.
        if let Some(name) = body.strip_prefix('-') {
            match name {
                "compatibility" => self.compatibility = true,
                "report-compatibility" => self.report_compatibility = true,
                "broken-sort-compatibility" => self.broken_sort = true,
                _ => warnings.push(format!("unrecognized option: --{}", name)),
            }
            return;
        }

        let mut cs = body.chars();
        let first = match cs.next() {
            Some(c) => c,
            None => return, // a bare "-"
        };
        let rest = &body[first.len_utf8()..];

        // 1.2: a numeric selector names the table, e.g. -51, -3210.
        if first.is_ascii_digit() {
            self.table = Some(body.to_string());
            return;
        }

        // 1.3: option letters are case-insensitive except -y, which exists
        // only in lower case.
        if first == 'Y' {
            warnings.push("unrecognized option: -Y".to_string());
            return;
        }
        match first.to_ascii_lowercase() {
            // 1.4: -a<xx> REPLACES the mask; a bare -a sets all four bits.
            'a' => self.strict = if rest.is_empty() { 0xFF } else { hex(rest) },
            'b' => {
                // 8.1: -b selects binary AND turns on block mode; a bare -g3
                // selects the format only.
                self.format = ObjFormat::Binary;
                self.block = true;
            }
            'c' => self.block = true,
            'd' => self.defines.push(rest.to_string()),
            'e' => self.expand = true,
            'f' => self.fill = Some(hex(rest) as u8),
            'g' => {
                self.format = match rest.chars().next() {
                    Some('0') => ObjFormat::IntelHex,
                    Some('1') => ObjFormat::MosTech,
                    Some('2') => ObjFormat::SRecord,
                    Some('3') => ObjFormat::Binary,
                    Some('4') => ObjFormat::IntelWord,
                    _ => {
                        warnings.push(format!("unrecognized object format: -g{}", rest));
                        self.format
                    }
                }
            }
            'h' => self.hex_dump = true,
            'i' => self.ignore_case = true,
            'l' => {
                self.labels = match rest.chars().next().map(|c| c.to_ascii_lowercase()) {
                    Some('l') => LabelTable::Long,
                    Some('a') => LabelTable::All,
                    _ => LabelTable::Short,
                }
            }
            'm' => self.format = ObjFormat::MosTech,
            'o' => self.bytes_per_record = hex(rest), // 8.2: HEX, so -o32 is 50
            'p' => self.page_lines = Some(dec(rest)),
            'q' => self.quiet = true,
            's' => self.symfile = true,
            't' => self.table = Some(rest.to_string()),
            // 1.3: a bare -x enables ALL classes; -x<d> sets the mask to the
            // single hex digit <d>. When -x is absent the mask is 1.
            'x' => {
                self.class_mask = match rest.chars().next().and_then(|c| c.to_digit(16)) {
                    Some(d) => d,
                    None => 0xFF,
                }
            }
            'y' => self.timing = true,
            'z' => self.debug = true,
            _ => warnings.push(format!("unrecognized option: -{}", body)),
        }
    }

    pub fn source(&self) -> Option<&str> {
        self.files.first().map(|s| s.as_str())
    }

    /// 1.6: output names default from the base name, which is the source name
    /// truncated at its FIRST '.', not its last, and capped at 79 characters.
    /// Returns the name and, if truncation happened, a warning for the caller.
    pub fn base_name(&self) -> (String, Option<String>) {
        let src = self.source().unwrap_or("");
        let base = match src.find('.') {
            Some(i) => &src[..i],
            None => src,
        };
        if base.len() > MAX_BASENAME {
            let cut: String = base.chars().take(MAX_BASENAME).collect();
            let warn = format!(
                "base file name longer than {} characters, truncated to {}",
                MAX_BASENAME, cut
            );
            (cut, Some(warn))
        } else {
            (base.to_string(), None)
        }
    }

    /// Positional output name if given, else base + `ext`.
    pub fn out_name(&self, slot: usize, ext: &str, base: &str) -> String {
        match self.files.get(slot) {
            Some(f) => f.clone(),
            None => format!("{}{}", base, ext),
        }
    }

    /// 1.5: TASMTABS names a single directory, not a search path, and is
    /// joined with a literal '/' on every platform. With it unset the bare
    /// name resolves against the working directory.
    pub fn table_path(&self, tasmtabs: Option<&str>) -> Option<String> {
        let sel = self.table.as_ref()?;
        let name = format!("tasm{}.tab", sel);
        Some(match tasmtabs {
            Some(dir) => format!("{}/{}", dir, name),
            None => name,
        })
    }
}

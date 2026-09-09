//! Command line, environment and file naming.
//!
//! Options come from the environment first and the command line second, so the
//! command line wins. Output names are derived from the source name when they
//! are not given.

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
    pub strict: u32,             // -a<xx>, hex bit mask (1.4)
    pub format: ObjFormat,       // -g<x>
    pub block: bool,             // -c, contiguous block output
    pub defines: Vec<String>,    // -d<macro>
    pub expand: bool,            // -e, list macro-expanded source
    pub fill: Option<u8>,        // -f<xx>, pre-fill the image
    pub hex_dump: bool,          // -h
    pub ignore_case: bool,       // -i
    pub labels: LabelTable,      // -l, -ll, -la
    pub bytes_per_record: u32,   // -o<xx>, HEX (8.2)
    pub page_lines: Option<u32>, // -p<lines>
    pub quiet: bool,             // -q, suppress the listing
    pub symfile: bool,           // -s
    pub class_mask: u32,         // -x<xx>, hex, default 1 (5.4 CLASS)
    pub timing: bool,            // -y, lower case only (1.3)
    pub debug: bool,             // -z, trace to stderr
    pub compatibility: bool,     // --compatibility (1.3, 3.1, 4.7)
    /// --message-prefix: the name on the assembler's own messages. TASM wrote
    /// `tasm:`; reproducing that byte-for-byte is a compatibility concern, so it
    /// is a value rather than a hidden behaviour.
    pub message_prefix: String,
    /// --page-title: the paged-listing heading used when the source sets no
    /// `.TITLE`.
    pub page_title: String,
    /// --bug-compatibility: reproduce the original's defects rather than the
    /// corrected behaviour -- the early-stopping symbol sort (2.1) and the
    /// word-address checksum (8.7). Distinct from --compatibility, which
    /// restores two *semantic* behaviours, and from --report-compatibility,
    /// which restores identity strings.
    pub bug_compatibility: bool,
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
            message_prefix: "tabasm".to_string(),
            page_title: "tabasm".to_string(),
            bug_compatibility: false,
            files: Vec::new(),
        }
    }
}

/// Parse a leading run of hex digits, returning 0 for an empty field. Values
/// are bounded rather than allowed to overflow: a 200-digit opcode field is one
/// of the hostile inputs testing/robustness.rs feeds us.
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

/// The long options that take a value, in either spelling. Kept as one list so
/// that adding an option cannot leave the two spellings out of step.
const LONG_WITH_VALUE: [&str; 3] = ["cpu", "message-prefix", "page-title"];

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

        let mut i = 0;
        while i < args.len() {
            let arg = args[i].clone();
            if let Some(body) = arg.strip_prefix('-') {
                // A long option that takes a value may be written either way:
                // `--cpu=z80` or `--cpu z80`. Only this loop can see the next
                // argument, so the separated form is joined here and handed on
                // as though it had been written with the '='.
                if let Some(name) = body.strip_prefix('-') {
                    if !name.contains('=') && LONG_WITH_VALUE.contains(&name) {
                        match args.get(i + 1) {
                            // Consumed whatever it is, as getopt would. A value
                            // may legitimately begin with '-': a page title, for
                            // one.
                            Some(value) => {
                                o.option(&format!("-{}={}", name, value), &mut warnings);
                                i += 2;
                                continue;
                            }
                            None => {
                                warnings.push(format!("option --{} needs a value", name));
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
                o.option(body, &mut warnings);
            } else if o.files.len() < MAX_FILE_ARGS {
                o.files.push(arg.clone());
            } else {
                // 10.3: non-fatal, the extra name is ignored.
                warnings.push(format!(
                    "too many file names (max {}): {}",
                    MAX_FILE_ARGS, arg
                ));
            }
            i += 1;
        }
        Parsed { opts: o, warnings }
    }

    fn option(&mut self, body: &str, warnings: &mut Vec<String>) {
        // The two long options. --report-compatibility is ours, not the
        // original's: it keeps the "tasm:" message prefix while the binary is
        // named tabasm, so the golden corpus stays byte-comparable.
        if let Some(name) = body.strip_prefix('-') {
            // Long options taking a value arrive here as `--name=value`;
            // `parse` rewrites the separated form into this one first.
            if let Some((key, value)) = name.split_once('=') {
                match key {
                    "cpu" => self.table = Some(value.to_string()),
                    "message-prefix" => self.message_prefix = value.to_string(),
                    "page-title" => self.page_title = value.to_string(),
                    _ => warnings.push(format!("unrecognized option: --{}", key)),
                }
                return;
            }
            match name {
                "compatibility" => self.compatibility = true,
                "bug-compatibility" => self.bug_compatibility = true,
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
                self.class_mask = rest
                    .chars()
                    .next()
                    .and_then(|c| c.to_digit(16))
                    .unwrap_or(0xFF)
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

    /// Where to look for the selected table, in order.
    ///
    /// `<name>.tab2` is the current format, named exactly as `--cpu` selects it;
    /// `tasm<name>.tab` is the legacy layout, still read so existing tables keep
    /// working. TASMTABS names a single directory, not a search path, and is
    /// joined with a literal '/' on every platform; with it unset the bare names
    /// resolve against the working directory.
    pub fn table_paths(&self, tasmtabs: Option<&str>) -> Vec<String> {
        let sel = match self.table.as_ref() {
            Some(s) => s,
            None => return Vec::new(),
        };
        [format!("{}.tab2", sel), format!("tasm{}.tab", sel)]
            .into_iter()
            .map(|name| match tasmtabs {
                Some(dir) => format!("{}/{}", dir, name),
                None => name,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> Options {
        Options::parse(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            None,
        )
        .opts
    }

    #[test]
    fn cpu_selects_the_table_and_accepts_alphanumeric_names() {
        assert_eq!(opts(&["--cpu=z80"]).table.as_deref(), Some("z80"));
        assert_eq!(
            opts(&["--cpu=tms320c25"]).table.as_deref(),
            Some("tms320c25")
        );
        assert_eq!(opts(&["--cpu=8051"]).table.as_deref(), Some("8051"));
        // The legacy forms still work, undocumented, so one source tree serves
        // both the published branch and the reference comparison.
        assert_eq!(opts(&["-51"]).table.as_deref(), Some("51"));
        assert_eq!(opts(&["-tz80"]).table.as_deref(), Some("z80"));
    }

    #[test]
    fn candidate_table_paths_are_tried_current_format_first() {
        let o = opts(&["--cpu=z80"]);
        assert_eq!(o.table_paths(None), ["z80.tab2", "tasmz80.tab"]);
        assert_eq!(o.table_paths(Some("/t")), ["/t/z80.tab2", "/t/tasmz80.tab"]);
        // 1.5: a single directory joined with a literal '/', not a search path.
        assert!(opts(&[]).table_paths(Some("/t")).is_empty());
    }

    #[test]
    fn the_identity_options_are_values_not_modes() {
        assert_eq!(opts(&[]).message_prefix, "tabasm");
        assert_eq!(opts(&[]).page_title, "tabasm");
        let o = opts(&["--message-prefix=tasm", "--page-title=Some Vendor."]);
        assert_eq!(o.message_prefix, "tasm");
        assert_eq!(o.page_title, "Some Vendor.");
    }

    #[test]
    fn a_long_option_takes_its_value_either_spelled_way() {
        // README uses the separated form throughout, so it has to work; the
        // '=' form is what the parser reduces everything to internally.
        for a in [vec!["--cpu", "z80"], vec!["--cpu=z80"]] {
            assert_eq!(opts(&a).table.as_deref(), Some("z80"), "{:?}", a);
        }
        let o = opts(&["--message-prefix", "tasm", "--page-title", "Some Vendor."]);
        assert_eq!(o.message_prefix, "tasm");
        assert_eq!(o.page_title, "Some Vendor.");

        // The whole README line, which is the one users will paste.
        let o = opts(&[
            "--compatibility",
            "--bug-compatibility",
            "--message-prefix",
            "tasm",
            "--cpu",
            "8051",
            "x.asm",
        ]);
        assert!(o.compatibility && o.bug_compatibility);
        assert_eq!(o.message_prefix, "tasm");
        assert_eq!(o.table.as_deref(), Some("8051"));
        assert_eq!(o.files, ["x.asm"]);
    }

    #[test]
    fn a_separated_value_is_consumed_and_never_read_as_a_file_name() {
        // The value must not fall through to the positional list -- that would
        // turn `--cpu z80 x.asm` into an assembly of "z80".
        let o = opts(&["--cpu", "z80", "x.asm"]);
        assert_eq!(o.files, ["x.asm"]);
        // Consumed as getopt would, even when it looks like an option: a page
        // title may legitimately begin with '-'.
        assert_eq!(opts(&["--page-title", "-x"]).page_title, "-x");
        // Trailing option with nothing after it: diagnosed, not swallowed.
        let p = Options::parse(&["--cpu".to_string()], None);
        assert!(p.warnings.iter().any(|w| w.contains("needs a value")));
        assert_eq!(p.opts.table, None);
    }

    #[test]
    fn the_two_compatibility_switches_are_independent() {
        let o = opts(&["--compatibility"]);
        assert!(o.compatibility && !o.bug_compatibility);
        let o = opts(&["--bug-compatibility"]);
        assert!(!o.compatibility && o.bug_compatibility);
    }
}

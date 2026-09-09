//! Conformance: check the assembler against things that have an answer
//! *outside* it — the shipped tables, checksum arithmetic, documented values,
//! and its own two renderings of the same assembly.
//!
//! **This file deliberately shares no code with `src/`.** It carries its own
//! readers for the table format, for object records and for listing lines. That
//! duplication is the point: a suite that parsed tables with the assembler's own
//! parser would agree with it about a misparse, and prove nothing. Where
//! `testing/golden.rs` asks "the same as yesterday?", this asks "right at all?".

mod common;
use common::*;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// An independent reader for the table format. Naive on purpose.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Row {
    mnemonic: String,
    operands: String,
    opcode: String,
    op_bytes: usize,
    arg_bytes: usize,
    rule: String,
}

#[derive(Default)]
struct Table {
    banner: String,
    ms_first: bool,
    word_addr: bool,
    aux_registers: u32,
    regsets: Vec<String>,
    rows: Vec<Row>,
}

fn read_table(path: &std::path::Path) -> Table {
    let text = std::fs::read_to_string(path).expect("table");
    let mut t = Table { aux_registers: 2, ..Default::default() };
    for line in text.lines() {
        let line = line.split(';').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('%') {
            let mut it = rest.split_whitespace();
            match it.next().unwrap_or("") {
                "banner" => {
                    t.banner = rest.splitn(3, '"').nth(1).unwrap_or("").to_string();
                }
                "opcode-order" => t.ms_first = it.next() == Some("ms-first"),
                "address-unit" => t.word_addr = it.next() == Some("word"),
                "aux-registers" => t.aux_registers = it.next().unwrap_or("2").parse().unwrap_or(2),
                "regset" => {
                    t.regsets.push(rest.splitn(3, '"').nth(1).unwrap_or("").to_string());
                }
                _ => {}
            }
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 6 {
            continue;
        }
        t.rows.push(Row {
            mnemonic: f[0].to_string(),
            operands: f[1].to_string(),
            opcode: f[2].to_string(),
            op_bytes: f[3].parse().unwrap_or(0),
            arg_bytes: f[4].parse().unwrap_or(0),
            rule: f[5].to_string(),
        });
    }
    t
}

// ---------------------------------------------------------------------------
// Independent readers for the two output renderings.
// ---------------------------------------------------------------------------

/// A listing line, as the layout describes it: a 24-column prefix of line
/// number, include depth, address, skip marker and up to four bytes, then the
/// source text.
struct Listed {
    line_no: u32,
    addr: u32,
    bytes: Vec<u8>,
    source: String,
}

fn read_listing(text: &str) -> Vec<Listed> {
    let mut out: Vec<Listed> = Vec::new();
    for l in text.lines() {
        if l.len() < 12 || !l.as_bytes()[..4].iter().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let line_no: u32 = l[..4].parse().unwrap();
        let addr = match u32::from_str_radix(l[7..11].trim(), 16) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let field = if l.len() > 24 { &l[12..24] } else { &l[12..] };
        let bytes: Vec<u8> = field
            .split_whitespace()
            .filter_map(|b| u8::from_str_radix(b, 16).ok())
            .collect();
        let source = if l.len() > 24 { l[24..].to_string() } else { String::new() };
        // A continuation repeats the line number with no source text; fold its
        // bytes into the line that started them.
        match out.last_mut() {
            Some(prev) if prev.line_no == line_no && source.is_empty() => {
                prev.bytes.extend(bytes);
            }
            _ => out.push(Listed { line_no, addr, bytes, source }),
        }
    }
    out
}

/// Every address the object file claims to load, and with what.
fn read_object(text: &str) -> BTreeMap<u32, u8> {
    let mut map = BTreeMap::new();
    for l in text.lines() {
        let l = l.trim();
        let (addr, data) = if let Some(r) = l.strip_prefix(':') {
            if r.len() < 10 || &r[6..8] != "00" {
                continue; // the terminator record
            }
            let n = usize::from_str_radix(&r[..2], 16).unwrap_or(0);
            (u32::from_str_radix(&r[2..6], 16).unwrap_or(0), &r[8..8 + n * 2])
        } else if let Some(r) = l.strip_prefix("S1") {
            let n = usize::from_str_radix(&r[..2], 16).unwrap_or(3) - 3;
            (u32::from_str_radix(&r[2..6], 16).unwrap_or(0), &r[6..6 + n * 2])
        } else if let Some(r) = l.strip_prefix(';') {
            if r.len() < 6 {
                continue;
            }
            let n = usize::from_str_radix(&r[..2], 16).unwrap_or(0);
            if n == 0 {
                continue;
            }
            (u32::from_str_radix(&r[2..6], 16).unwrap_or(0), &r[6..6 + n * 2])
        } else {
            continue;
        };
        for (i, pair) in data.as_bytes().chunks(2).enumerate() {
            let b = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
            map.insert(addr + i as u32, b);
        }
    }
    map
}

// ---------------------------------------------------------------------------

const TARGETS: &[&str] = &[
    "8048", "6502", "6800", "6805", "8051", "8085", "8096", "z80", "tms7000", "tms32010",
    "tms320c25",
];

/// Rules that shorten the instruction when the operand fits one byte. For these
/// the table declares the long form, so a large operand is needed before the
/// declared length can be asserted.
fn can_shorten(rule: &str) -> bool {
    matches!(
        rule,
        "zero-page"
            | "zero-page-moto"
            | "zero-page-st7"
            | "i8096-short-long-1"
            | "i8096-short-long-2"
            | "i8096-short-long-3"
            | "i8096-indexed"
    )
}

/// Build a source line for a row: literals copied, `<expr>` given a value,
/// `<reg>` given a declared register-set name.
fn synthesise(row: &Row, t: &Table, value: &str) -> String {
    let mut out = String::new();
    let mut rest = row.operands.as_str();
    if rest == "-" {
        return format!("        {}", row.mnemonic);
    }
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<expr>") {
            out.push_str(value);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("<reg>") {
            out.push_str(t.regsets.first().map(|s| s.as_str()).unwrap_or("*"));
            rest = r;
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    format!("        {} {}", row.mnemonic, out)
}

/// Every row a table declares must be reachable, must emit the length it
/// declares, and — where the rule leaves the opcode alone — must emit the opcode
/// it declares. The table states all three answers, so none of this is circular.
#[test]
fn every_table_row_is_reachable_and_encodes_as_declared() {
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for target in TARGETS {
        let t = read_table(&root().join(format!("tables/{}.tab2", target)));

        // First match wins, so a repeated (mnemonic, operands) pair can only ever
        // reach the earliest row. Sweep the distinct patterns.
        let mut first: Vec<&Row> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in &t.rows {
            if seen.insert((r.mnemonic.clone(), r.operands.clone())) {
                first.push(r);
            }
        }

        // A large operand so nothing shortens, keeping the declared length the
        // one to expect.
        let scratch = Scratch::new("sweep");
        let mut src = String::from("        .org 0\n");
        for r in &first {
            src.push_str(&synthesise(r, &t, "$1234"));
            src.push('\n');
        }
        src.push_str("        .end\n");
        let path = scratch.path("sweep.asm");
        std::fs::write(&path, &src).unwrap();

        let run = assemble(
            &[&format!("--cpu={}", target), "-x"],
            path.to_str().unwrap(),
            &scratch,
            &[],
        );
        let listing = read_listing(&run.text("lst"));

        // Line 1 is .org, so row i is listing line i + 2.
        for (i, r) in first.iter().enumerate() {
            let want_line = i as u32 + 2;
            let listed = listing.iter().find(|l| l.line_no == want_line);
            let listed = match listed {
                Some(l) => l,
                None => {
                    problems.push(format!("{} {}: no listing line", target, r.mnemonic));
                    continue;
                }
            };
            checked += 1;

            if listed.bytes.is_empty() {
                problems.push(format!(
                    "{} {:<8} {:<20} unreachable: matched no row",
                    target, r.mnemonic, r.operands
                ));
                continue;
            }
            let declared = r.op_bytes + r.arg_bytes;
            if !can_shorten(&r.rule) && listed.bytes.len() != declared {
                problems.push(format!(
                    "{} {:<8} {:<20} emitted {} bytes, table declares {}",
                    target,
                    r.mnemonic,
                    r.operands,
                    listed.bytes.len(),
                    declared
                ));
                continue;
            }
            // A `<reg>` slot folds the register-set mask into the opcode, so
            // the declared value is not what should be emitted. Those rows are
            // still covered by the reachability and length checks above.
            if r.rule == "plain" && !r.operands.contains("<reg>") {
                let want: Vec<u8> = r
                    .opcode
                    .as_bytes()
                    .chunks(2)
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                    .collect();
                let want = if t.ms_first {
                    want.clone()
                } else {
                    want.iter().rev().cloned().collect()
                };
                if listed.bytes[..want.len().min(listed.bytes.len())] != want[..] {
                    problems.push(format!(
                        "{} {:<8} {:<20} opcode {:02X?}, table declares {} ({})",
                        target,
                        r.mnemonic,
                        r.operands,
                        &listed.bytes[..want.len().min(listed.bytes.len())],
                        r.opcode,
                        if t.ms_first { "ms-first" } else { "ls-first" }
                    ));
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "{} of {} rows disagree with their table:\n  {}",
        problems.len(),
        checked,
        problems.iter().take(40).cloned().collect::<Vec<_>>().join("\n  ")
    );
    assert!(checked > 2000, "expected to sweep the whole table set, got {}", checked);
    eprintln!("{} distinct table rows checked against their declarations", checked);
}

/// Checksums are re-derived here, not remembered. A record whose checksum does
/// not close is one a conforming loader would reject.
#[test]
fn every_object_record_carries_a_correct_checksum() {
    for target in TARGETS {
        for fmt in ["-g0", "-g1", "-g2", "-g4"] {
            let scratch = Scratch::new("cksum");
            let src = format!("testing/smoke/{}.asm", target);
            let run = assemble(&[&format!("--cpu={}", target), "-x", fmt], &src, &scratch, &[]);
            let obj = run.text("obj");
            let mut records = 0;
            for l in obj.lines() {
                let l = l.trim();
                let bytes: Vec<u8> = match l.chars().next() {
                    Some(':') => hex_bytes(&l[1..]),
                    Some('S') => hex_bytes(&l[2..]),
                    Some(';') if l.len() > 3 => hex_bytes(&l[1..]),
                    _ => continue,
                };
                if bytes.len() < 2 {
                    continue;
                }
                records += 1;
                match fmt {
                    // Intel: the whole record, checksum included, sums to zero.
                    "-g0" | "-g4" => {
                        let sum: u32 = bytes.iter().map(|b| *b as u32).sum();
                        assert_eq!(
                            sum & 0xFF, 0,
                            "{} {}: record does not close: {}", target, fmt, l
                        );
                    }
                    // Motorola: the checksum is the complement of the sum of the
                    // count, address and data bytes.
                    "-g2" => {
                        let (body, ck) = bytes.split_at(bytes.len() - 1);
                        let sum: u32 = body.iter().map(|b| *b as u32).sum();
                        assert_eq!(
                            (!sum) & 0xFF, ck[0] as u32,
                            "{} {}: bad S-record checksum: {}", target, fmt, l
                        );
                    }
                    // MOS Technology: a plain 16-bit sum, four hex digits, not
                    // complemented.
                    _ => {
                        if l == ";00" {
                            continue;
                        }
                        let (body, ck) = bytes.split_at(bytes.len() - 2);
                        let sum: u32 = body.iter().map(|b| *b as u32).sum();
                        let carried = ((ck[0] as u32) << 8) | ck[1] as u32;
                        assert_eq!(
                            sum & 0xFFFF, carried,
                            "{} {}: bad MOS checksum: {}", target, fmt, l
                        );
                    }
                }
            }
            assert!(records > 1, "{} {}: no records emitted", target, fmt);
        }
    }
}

fn hex_bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

/// The object file and the listing are two renderings of one assembly. They
/// cannot disagree about what byte belongs at an address.
#[test]
fn the_object_file_and_the_listing_agree() {
    for target in TARGETS {
        let scratch = Scratch::new("agree");
        let src = format!("testing/smoke/{}.asm", target);
        let run = assemble(&[&format!("--cpu={}", target), "-x"], &src, &scratch, &[]);
        let from_obj = read_object(&run.text("obj"));
        let t = read_table(&root().join(format!("tables/{}.tab2", target)));

        let mut from_lst: BTreeMap<u32, u8> = BTreeMap::new();
        for l in read_listing(&run.text("lst")) {
            // With word addressing the listing counts words while the object
            // counts bytes.
            let base = if t.word_addr { l.addr * 2 } else { l.addr };
            for (i, b) in l.bytes.iter().enumerate() {
                from_lst.insert(base + i as u32, *b);
            }
        }
        assert!(!from_obj.is_empty(), "{}: object file had no data", target);

        // Every byte the listing shows must appear at the same address in the
        // object. The converse does not hold under word addressing: an odd byte
        // count rounds up to a whole word, and that pad byte is part of the
        // emitted region but is never listed.
        for (addr, byte) in &from_lst {
            assert_eq!(
                from_obj.get(addr),
                Some(byte),
                "{}: listing shows {:02X} at {:04X}, object says {:02X?}",
                target,
                byte,
                addr,
                from_obj.get(addr)
            );
        }
        if !t.word_addr {
            assert_eq!(
                from_obj, from_lst,
                "{}: byte-addressed targets have no padding, so the two must match exactly",
                target
            );
        } else {
            let pad = from_obj.len() - from_lst.len();
            assert!(
                pad <= from_lst.len(),
                "{}: {} pad bytes is more than the listing accounts for",
                target,
                pad
            );
        }
    }
}

/// Values the manual states, which are arithmetic rather than observation.
#[test]
fn documented_expression_values_hold() {
    let cases: &[(&str, &str, i64)] = &[
        ("1+2*3+4", "", 11),
        ("1+2*(3+4)", "", 15),
        ("2+3*4", "", 14),
        ("10-2-3", "", 5),
        ("1<<2+3", "", 32),
        ("$1F", "", 31),
        ("%1010", "", 10),
        ("@17", "", 15),
        ("0FFh", "", 255),
        ("1010b", "", 10),
        ("0107q", "", 71),
        ("'A'", "", 65),
        ("17%5", "", 2),
        ("~0", "", 0xFF),
        ("!0", "", 1),
        ("-1", "", 0xFF),
        // No precedence under --compatibility: strictly left to right.
        ("1+2*3+4", "--compatibility", 13),
        ("1+2*(3+4)", "--compatibility", 21),
        ("2+3*4", "--compatibility", 20),
    ];
    for (expr, mode, want) in cases {
        let scratch = Scratch::new("expr");
        let path = scratch.path("e.asm");
        std::fs::write(&path, format!("        .org 0\n        .byte {}\n        .end\n", expr))
            .unwrap();
        let mut args = vec!["--cpu=8051"];
        if !mode.is_empty() {
            args.push(mode);
        }
        let run = assemble(&args, path.to_str().unwrap(), &scratch, &[]);
        let got = read_object(&run.text("obj"));
        assert_eq!(
            got.get(&0).copied().map(|b| b as i64),
            Some(*want & 0xFF),
            "`{}` {} should be {}",
            expr,
            mode,
            want
        );
    }
}

/// `--bug-compatibility` restores two defects and must touch nothing else.
#[test]
fn bug_compatibility_changes_only_what_it_claims() {
    // The word-address checksum: corrected records close, the original's do not.
    for mode in [false, true] {
        let scratch = Scratch::new("bug");
        let mut args = vec!["--cpu=8051", "-x", "-g4"];
        if mode {
            args.push("--bug-compatibility");
        }
        let run = assemble(&args, "testing/smoke/8051.asm", &scratch, &[]);
        let bad = run
            .text("obj")
            .lines()
            .filter(|l| l.starts_with(':') && !l.starts_with(":00000001"))
            .filter(|l| hex_bytes(&l[1..]).iter().map(|b| *b as u32).sum::<u32>() & 0xFF != 0)
            .count();
        if mode {
            assert!(bad > 0, "--bug-compatibility should restore the defective checksum");
        } else {
            assert_eq!(bad, 0, "by default every -g4 record must close");
        }
    }

    // Everything that is not the label order or a -g4 checksum is unaffected.
    for target in TARGETS {
        let src = format!("testing/smoke/{}.asm", target);
        let a = Scratch::new("bugA");
        let b = Scratch::new("bugB");
        let plain = assemble(&[&format!("--cpu={}", target), "-x"], &src, &a, &[]);
        let bug = assemble(
            &[&format!("--cpu={}", target), "-x", "--bug-compatibility"],
            &src,
            &b,
            &[],
        );
        assert_eq!(plain.obj, bug.obj, "{}: default object format must not change", target);
        assert_eq!(plain.out, bug.out, "{}: standard output must not change", target);
        assert_eq!(plain.code, bug.code, "{}: exit status must not change", target);
    }
}

/// Layout invariants the listing must always satisfy.
#[test]
fn listing_structure_is_invariant() {
    for target in TARGETS {
        let scratch = Scratch::new("layout");
        let src = format!("testing/smoke/{}.asm", target);
        let run = assemble(&[&format!("--cpu={}", target), "-x"], &src, &scratch, &[]);
        let text = run.text("lst");
        for l in text.lines() {
            if l.len() < 4 || !l.as_bytes()[..4].iter().all(|c| c.is_ascii_digit()) {
                continue; // a diagnostic or the error count
            }
            // Four bytes per line at most, and the prefix is 24 columns wide, so
            // any source text starts there.
            let field = if l.len() > 24 { &l[12..24] } else { &l[12..] };
            let n = field.split_whitespace().count();
            assert!(n <= 4, "{}: more than four bytes on one line: {:?}", target, l);
            if l.len() > 24 {
                assert_eq!(&l[11..12], " ", "{}: column 11 is the skip marker: {:?}", target, l);
            }
        }
    }
}

/// The CRLF case must actually contain CRLF. It is the only fixture whose
/// bytes, rather than its text, are the point -- and an editor, a checkout or a
/// well-meaning script can normalise it away without any test noticing, because
/// the recorded output would simply be re-recorded to match.
#[test]
fn the_crlf_fixture_still_has_crlf_line_endings() {
    let bytes = std::fs::read(root().join("testing/cases/crlf-source.asm")).expect("fixture");
    let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
    let bare = bytes.iter().filter(|b| **b == b'\n').count() - crlf;
    assert!(crlf > 10, "expected CRLF line endings, found {}", crlf);
    assert_eq!(bare, 0, "{} lines lost their carriage return", bare);
}

/// Exit status is part of the interface: 0 clean, 1 diagnosed, 3 unopenable.
#[test]
fn exit_status_reflects_the_outcome() {
    let scratch = Scratch::new("exit");
    let clean = assemble(&["--cpu=8051", "-x"], "testing/smoke/8051.asm", &scratch, &[]);
    assert_eq!(clean.code, Some(0), "a clean assembly exits 0");

    let errs = assemble(&["--cpu=8051"], "testing/cases/err-undef.asm", &scratch, &[]);
    assert_eq!(errs.code, Some(1), "a diagnosed assembly exits 1");

    let missing = assemble(&["--cpu=nosuchcpu"], "testing/smoke/8051.asm", &scratch, &[]);
    assert_eq!(missing.code, Some(3), "an unopenable table exits 3");
}

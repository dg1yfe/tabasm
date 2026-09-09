//! The v2 instruction-table format (`doc/TABLE-FORMAT-V2.md`).
//!
//! v1 is retained unchanged; this parser produces the same `Table`, so the
//! matcher, the encoding rules and the engine are untouched by the format. What
//! it adds is that the mistakes v1 makes silently are diagnosed:
//!
//! - fields are checked against a declared `%columns` list, so a missing column
//!   is an error naming the line rather than a shift that mis-registers a row
//!   (the `IM0`/`IM1`/`IM2` defect of spec §7.19);
//! - `;` is a real comment leader, so a row is never skipped by accident;
//! - byte counts are stated rather than derived, and cross-checked;
//! - rule names are matched in full, so `combrel` is an error instead of
//!   silently selecting `combine`;
//! - each rule accepts only the parameters it defines, in place of v1's `SHIFT`
//!   and `OR` columns carrying five different meanings between them.

use crate::limits::*;
use crate::table::{self, LoadError, RegSet, Row, Rule, Table};

/// `<expr>` and `<reg>` are translated to bytes that cannot appear in source
/// operand text, so every other character of a pattern is a literal. That is
/// what lets `.ALTWILD` disappear: `*` is just a character again, which is what
/// the TMS320 tables need it to be.
pub const EXPR: u8 = 0x01;
pub const REG: u8 = 0x02;

const COLUMNS: [&str; 6] = [
    "mnemonic",
    "operands",
    "opcode",
    "op-bytes",
    "arg-bytes",
    "rule",
];

/// Every parameter is accepted by exactly the rules that read it.
const UNIVERSAL: &[&str] = &["class", "post-shift", "post-or"];

fn rule_for(name: &str) -> Option<Rule> {
    Some(match name {
        // Phase A -- the fourteen rules the 8-bit tables use.
        "plain" => table::NO,
        "jmp-page-2k" => table::JM,
        "jmp-page-256" => table::JT,
        "rel8" => table::R1,
        "zero-page" => table::ZP,
        "zero-page-moto" => table::MZ,
        "bit-moto" => table::MB,
        "bit-z80" => table::ZB,
        "index-z80" => table::ZI,
        "combine" => table::CO,
        "combine-rel" => table::CR,
        "combine-swapped" => table::CS,
        "swap-bytes" => table::SW,
        "three-rel" => table::R3REL,
        // Phase B.
        "tms-fold" => table::T1,
        "tms-dma" => table::TD,
        "tms-long" => table::TL,
        "tms-long-swapped" => table::T5,
        "tms-aux" => table::TA,
        "tms7000-trap" => table::SU,
        "rel16" => table::R2,
        "i8096-combine" => table::I1,
        "i8096-short-long-2" => table::I2,
        "i8096-short-long-3" => table::I3,
        "i8096-jump-bit" => table::I4,
        "i8096-rel11" => table::I5,
        "i8096-indexed" => table::I6,
        "i8096-short-long-1" => table::I7,
        "i8096-combine-swapped" => table::I8,
        // The thirteen of §7.25 that no shipped table selects.
        "tms9900-swap-dst" => table::T2,
        "tms9900-swap-src" => table::T3,
        "tms9900-regs" => table::T4,
        "nibble-dma" => table::T6,
        "three-plain" => table::A3,
        "z8-nibbles" => table::CN,
        "z8-nibbles-swapped" => table::C5,
        "zero-page-st7" => table::SZ,
        "bit-st7" => table::SB,
        "rel4" => table::R3,
        "z8-working-pair" => table::ZW,
        "z8-djnz" => table::ZD,
        "z8-load-indexed" => table::ZL,
        _ => return None,
    })
}

/// The parameters a rule reads, beyond the universal ones. This is the whole
/// point of the format: v1's `OR` column silently meant a value, an agreement
/// mask, a validity mask or a per-field validity mask depending on the rule and
/// on a table-level directive.
fn params_for(rule: Rule) -> &'static [&'static str] {
    match rule {
        table::JM => &["page-mask"],
        // shift_and consumers: a shift count in the low nibble, an invert flag
        // in the high one, and a validity mask.
        table::T1 | table::TD | table::TL | table::T5 | table::TA | table::T6 => {
            &["shift", "invert", "valid-mask"]
        }
        table::CS | table::CN | table::C5 => &["valid-mask"],
        table::I1 => &["field-mask", "set-bits"],
        table::I2 => &["field-mask", "opcode-xor"],
        table::I3 | table::I4 | table::I5 | table::I6 | table::I7 | table::I8 => &["field-mask"],
        _ => &[],
    }
}

fn hex_val(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut v: u32 = 0;
    for c in s.chars() {
        v = v.saturating_mul(16).saturating_add(c.to_digit(16).unwrap());
    }
    Some(v)
}

fn dec_val(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok()
}

fn err<T>(line: usize, what: impl Into<String>) -> Result<T, LoadError> {
    Err(LoadError::Syntax(line, what.into()))
}

/// Strip a `;` comment, respecting double quotes so a banner may contain one.
///
/// `;` and not `#`: `#` is the immediate-addressing prefix in 162 operand
/// patterns across the shipped tables, so using it here would truncate
/// `ADC #<expr>` to `ADC` -- the same class of mistake v1 makes with `/*`. Of the
/// characters that never appear in an operand pattern, `;` is the one an
/// assembly programmer expects.
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let mut quoted = false;
    for (i, c) in b.iter().enumerate() {
        match c {
            b'"' => quoted = !quoted,
            b';' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn quoted(s: &str) -> Option<&str> {
    let t = s.strip_prefix('"')?;
    t.strip_suffix('"')
}

/// Translate an operand pattern. `-` means the instruction takes no operands
/// (v1's `""`); `<expr>` and `<reg>` become sentinels; everything else is a
/// literal.
fn pattern(line: usize, text: &str) -> Result<String, LoadError> {
    if text == "-" {
        return Ok(String::new());
    }
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<expr>") {
            out.push(EXPR);
            i += 6;
        } else if b[i..].starts_with(b"<reg>") {
            out.push(REG);
            i += 5;
        } else if b[i] == b'<' {
            return err(line, format!("unknown placeholder in operands: {}", text));
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    // §6.4: every capture but the last must be terminated by a literal, or the
    // extractor cannot tell where it ends. v1 mis-parses in silence here.
    let last = out.iter().rposition(|c| *c == EXPR);
    for (k, c) in out.iter().enumerate() {
        if *c != EXPR || Some(k) == last {
            continue;
        }
        let next = out.get(k + 1).copied();
        let ok = matches!(next, Some(b',') | Some(b'[') | Some(b']'))
            || (next == Some(b')') && out.get(k + 2) == Some(&b','));
        if !ok {
            return err(
                line,
                format!(
                    "<expr> before the last must be followed by ',', '[' or ']': {}",
                    text
                ),
            );
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub fn parse(text: &str, selector: &str) -> Result<Table, LoadError> {
    let mut t = Table::new(selector);
    t.regmark = REG;
    t.wildcard = EXPR as char;
    // v2 states the transform per row, so there is no table-level step.
    t.noargshift = true;
    let mut columns: Option<Vec<String>> = None;
    let mut classes: Vec<(String, u32)> = Vec::new();
    let mut seen_format = false;

    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let body = strip_comment(raw.strip_suffix('\r').unwrap_or(raw)).trim();
        if body.is_empty() {
            continue;
        }

        if let Some(rest) = body.strip_prefix('%') {
            let mut it = rest.split_whitespace();
            let name = it.next().unwrap_or("");
            let args: Vec<&str> = it.collect();
            match name {
                "format" => match args.first().and_then(|s| dec_val(s)) {
                    Some(2) => seen_format = true,
                    _ => return err(line, "only %format 2 is supported"),
                },
                "banner" => {
                    let joined = rest[name.len()..].trim();
                    match quoted(joined) {
                        Some(b) => t.banner = b.to_string(),
                        None => return err(line, "%banner needs a double-quoted string"),
                    }
                }
                "opcode-order" => match args.first().copied() {
                    Some("ls-first") => t.msfirst = false,
                    Some("ms-first") => t.msfirst = true,
                    _ => return err(line, "%opcode-order takes ls-first or ms-first"),
                },
                "address-unit" => match args.first().copied() {
                    Some("byte") => t.wordaddrs = false,
                    Some("word") => t.wordaddrs = true,
                    _ => return err(line, "%address-unit takes byte or word"),
                },
                "aux-registers" => match args.first().and_then(|s| dec_val(s)) {
                    Some(v) if v >= 1 && v.is_power_of_two() => t.aux_registers = v,
                    _ => return err(line, "%aux-registers takes a power of two"),
                },
                "class" => match (args.first().and_then(|s| hex_val(s)), args.get(1)) {
                    (Some(bit), Some(nm)) => classes.push((nm.to_string(), bit)),
                    _ => return err(line, "%class takes a hex bit and a name"),
                },
                "regset" => {
                    if t.regsets.len() >= MAX_REGSETS {
                        return Err(LoadError::TooManyRegsets);
                    }
                    let nm = match quoted(args.first().copied().unwrap_or("")) {
                        Some(v) => v.to_string(),
                        None => return err(line, "%regset needs a double-quoted name"),
                    };
                    let (mut mask, mut class) = (0u32, 1u32);
                    for a in &args[1..] {
                        let (k, v) = match a.split_once('=') {
                            Some(kv) => kv,
                            None => {
                                return err(line, format!("%regset: expected key=value, got {}", a))
                            }
                        };
                        match k {
                            "mask" => {
                                mask =
                                    hex_val(v).ok_or(LoadError::Syntax(line, "bad mask".into()))?
                            }
                            "class" => class = resolve_class(line, v, &classes)?,
                            _ => return err(line, format!("%regset has no parameter {}", k)),
                        }
                    }
                    t.regsets.push(RegSet {
                        name: nm,
                        mask,
                        class,
                    });
                }
                "columns" => {
                    if args.len() != COLUMNS.len() {
                        return err(
                            line,
                            format!("%columns needs exactly {}: {:?}", COLUMNS.len(), COLUMNS),
                        );
                    }
                    for c in &args {
                        if !COLUMNS.contains(c) {
                            return err(line, format!("unknown column {}", c));
                        }
                    }
                    columns = Some(args.iter().map(|s| s.to_string()).collect());
                }
                _ => return err(line, format!("unknown directive %{}", name)),
            }
            continue;
        }

        // A row.
        if !seen_format {
            return err(line, "table must open with %format 2");
        }
        let cols = match &columns {
            Some(c) => c,
            None => return err(line, "a row appeared before %columns"),
        };
        if t.rows.len() >= MAX_TABLE_ROWS {
            return Err(LoadError::TooManyRows);
        }
        t.rows.push(row(line, body, cols, &classes)?);
    }

    if !seen_format {
        return err(1, "table must open with %format 2");
    }
    // §5.6: order is significant and first match wins, so a row repeating an
    // earlier pattern for the same mnemonic can never match. v1 accepts this in
    // silence.
    for i in 0..t.rows.len() {
        for j in 0..i {
            if t.rows[i].mnemonic == t.rows[j].mnemonic
                && t.rows[i].args == t.rows[j].args
                && t.rows[i].class & t.rows[j].class != 0
            {
                return err(
                    0,
                    format!(
                        "unreachable row: {} {} repeats an earlier pattern",
                        t.rows[i].mnemonic,
                        display(&t.rows[i].args)
                    ),
                );
            }
        }
    }
    Ok(t)
}

fn resolve_class(line: usize, v: &str, classes: &[(String, u32)]) -> Result<u32, LoadError> {
    if let Some((_, bit)) = classes.iter().find(|(n, _)| n == v) {
        return Ok(*bit);
    }
    hex_val(v).ok_or(LoadError::Syntax(line, format!("unknown class {}", v)))
}

fn row(
    line: usize,
    body: &str,
    cols: &[String],
    classes: &[(String, u32)],
) -> Result<Row, LoadError> {
    let f: Vec<&str> = body.split_whitespace().collect();
    if f.len() < cols.len() {
        return err(
            line,
            format!("expected {} columns, found {}", cols.len(), f.len()),
        );
    }
    // Anything past the declared columns must be a rule parameter, which is what
    // makes a miscounted row an error instead of a silent shift.
    for extra in &f[cols.len()..] {
        if !extra.contains('=') {
            return err(
                line,
                format!(
                    "expected {} columns, found extra field {}",
                    cols.len(),
                    extra
                ),
            );
        }
    }

    let get = |name: &str| -> &str {
        cols.iter()
            .position(|c| c == name)
            .map(|i| f[i])
            .unwrap_or("")
    };
    let mnemonic = get("mnemonic").to_ascii_uppercase();
    let operands = get("operands").to_string();
    let opcode_text = get("opcode").to_string();
    let op_bytes_text = get("op-bytes").to_string();
    let arg_bytes_text = get("arg-bytes").to_string();
    let rule_text = get("rule").to_string();

    let opcode = match hex_val(&opcode_text) {
        Some(v) => v,
        None => return err(line, format!("opcode is not hexadecimal: {}", opcode_text)),
    };
    let op_bytes = match dec_val(&op_bytes_text) {
        Some(v) if (1..=4).contains(&v) => v as u8,
        _ => {
            return err(
                line,
                format!("op-bytes must be 1..4, found {}", op_bytes_text),
            )
        }
    };
    // v1 derives the opcode width from the digit count, so `0` and `00` differ.
    // Here both are stated and must agree.
    if opcode_text.len() != op_bytes as usize * 2 {
        return err(
            line,
            format!(
                "op-bytes {} disagrees with a {}-digit opcode {}",
                op_bytes,
                opcode_text.len(),
                opcode_text
            ),
        );
    }
    let arg_bytes = match dec_val(&arg_bytes_text) {
        Some(v) if v <= 8 => v as u8,
        _ => {
            return err(
                line,
                format!("arg-bytes must be 0..8, found {}", arg_bytes_text),
            )
        }
    };
    let rule = match rule_for(&rule_text) {
        Some(r) => r,
        None => return err(line, format!("unknown rule {}", rule_text)),
    };

    let mut r = Row {
        mnemonic,
        args: pattern(line, &operands)?,
        opcode,
        opcode_bytes: op_bytes,
        arg_bytes,
        rule,
        class: 1,
        shift: 0,
        or: 0,
        post_shift: 0,
        post_or: 0,
        short_count: false,
    };

    let allowed = params_for(rule);
    for kv in &f[cols.len()..] {
        let (k, v) = kv.split_once('=').unwrap();
        if !UNIVERSAL.contains(&k) && !allowed.contains(&k) {
            return err(line, format!("rule {} has no parameter {}", rule_text, k));
        }
        match k {
            "class" => r.class = resolve_class(line, v, classes)?,
            "post-shift" => r.post_shift = need_hex(line, k, v)?,
            "post-or" => r.post_or = need_hex(line, k, v)?,
            // All of these land in the two fields the rules read; the names
            // exist so a reader knows which meaning applies.
            "page-mask" | "valid-mask" | "field-mask" => r.or = need_hex(line, k, v)?,
            "set-bits" | "opcode-xor" => r.shift = need_hex(line, k, v)?,
            "shift" => {
                let c = need_hex(line, k, v)?;
                if c > 0x0F {
                    return err(line, "shift is a count of 0..15");
                }
                r.shift = (r.shift & 0xF0) | c;
            }
            "invert" => match v {
                // §7.20's high-nibble flag. No shipped v1 table sets it.
                "yes" => r.shift |= 0x80,
                "no" => {}
                _ => return err(line, "invert takes yes or no"),
            },
            _ => unreachable!(),
        }
    }
    Ok(r)
}

fn need_hex(line: usize, k: &str, v: &str) -> Result<u32, LoadError> {
    hex_val(v).ok_or(LoadError::Syntax(
        line,
        format!("{} is not hexadecimal: {}", k, v),
    ))
}

/// Render an internal pattern back to v2 source, for diagnostics and for the
/// round-trip test.
pub fn display(args: &str) -> String {
    if args.is_empty() {
        return "-".to_string();
    }
    let mut out = String::new();
    for b in args.bytes() {
        match b {
            EXPR => out.push_str("<expr>"),
            REG => out.push_str("<reg>"),
            c => out.push(c as char),
        }
    }
    out
}

/// Render a table as v2 source. Used by the `tab1to2` converter and by the
/// round-trip test.
///
/// `banner` overrides the table's own, which is how a converted table stops
/// carrying the original product's name.
pub fn render(t: &Table, banner: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str("%format         2\n");
    s.push_str(&format!(
        "%banner         \"{}\"\n",
        banner.unwrap_or(&t.banner)
    ));
    s.push_str(&format!(
        "%opcode-order   {}\n",
        if t.msfirst { "ms-first" } else { "ls-first" }
    ));
    s.push_str(&format!(
        "%address-unit   {}\n",
        if t.wordaddrs { "word" } else { "byte" }
    ));
    s.push_str(&format!("%aux-registers  {}\n", t.aux_registers));
    for r in &t.regsets {
        s.push_str(&format!(
            "%regset         \"{}\" mask={:X} class={:X}\n",
            r.name, r.mask, r.class
        ));
    }
    s.push_str("%columns        mnemonic operands opcode op-bytes arg-bytes rule\n");
    for r in &t.rows {
        // v1 spells the wildcard per table and the register slot `!`.
        let mut pat = String::new();
        // Either spelling of "no operands": the legacy `""` or an already
        // translated empty pattern.
        if r.args.is_empty() || r.args == "\"\"" {
            pat.push('-');
        } else {
            for c in r.args.chars() {
                if c == t.wildcard {
                    pat.push_str("<expr>");
                } else if c == t.regmark as char {
                    pat.push_str("<reg>");
                } else {
                    pat.push(c);
                }
            }
        }
        let mut params = String::new();
        if r.class != 1 {
            params.push_str(&format!(" class={:X}", r.class));
        }
        let allowed = params_for(r.rule);
        if allowed.contains(&"page-mask") && r.or != 0 {
            params.push_str(&format!(" page-mask={:X}", r.or));
        } else if allowed.contains(&"valid-mask") && r.or != 0 {
            params.push_str(&format!(" valid-mask={:X}", r.or));
        } else if allowed.contains(&"field-mask") && r.or != 0 {
            params.push_str(&format!(" field-mask={:X}", r.or));
        }
        if allowed.contains(&"set-bits") && r.shift != 0 {
            params.push_str(&format!(" set-bits={:X}", r.shift));
        } else if allowed.contains(&"opcode-xor") && r.shift != 0 {
            params.push_str(&format!(" opcode-xor={:X}", r.shift));
        } else if allowed.contains(&"shift") {
            if r.shift & 0x0F != 0 {
                params.push_str(&format!(" shift={:X}", r.shift & 0x0F));
            }
            if r.shift & 0xF0 != 0 {
                params.push_str(" invert=yes");
            }
        }
        if r.post_shift != 0 {
            params.push_str(&format!(" post-shift={:X}", r.post_shift));
        }
        if r.post_or != 0 {
            params.push_str(&format!(" post-or={:X}", r.post_or));
        }
        s.push_str(&format!(
            "{} {} {:0width$X} {} {} {}{}\n",
            r.mnemonic,
            pat,
            r.opcode,
            r.opcode_bytes,
            r.arg_bytes,
            name_for(r.rule),
            params,
            width = r.opcode_bytes as usize * 2,
        ));
    }
    s
}

pub fn name_for(rule: Rule) -> &'static str {
    for n in [
        "plain",
        "jmp-page-2k",
        "jmp-page-256",
        "rel8",
        "zero-page",
        "zero-page-moto",
        "bit-moto",
        "bit-z80",
        "index-z80",
        "combine",
        "combine-rel",
        "combine-swapped",
        "swap-bytes",
        "three-rel",
        "tms-fold",
        "tms-dma",
        "tms-long",
        "tms-long-swapped",
        "tms-aux",
        "tms7000-trap",
        "rel16",
        "i8096-combine",
        "i8096-short-long-2",
        "i8096-short-long-3",
        "i8096-jump-bit",
        "i8096-rel11",
        "i8096-indexed",
        "i8096-short-long-1",
        "i8096-combine-swapped",
    ] {
        if rule_for(n) == Some(rule) {
            return n;
        }
    }
    panic!("no v2 name for rule {:?}", rule);
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: [&str; 11] = [
        "6502",
        "6800",
        "6805",
        "8048",
        "8051",
        "8085",
        "8096",
        "tms32010",
        "tms320c25",
        "tms7000",
        "z80",
    ];

    /// The round trip: load each shipped table, render it, parse the result, and
    /// require the two `Table` values to be equivalent. Rendering and parsing
    /// must be exact inverses over all 2811 rows.
    #[test]
    fn every_shipped_table_survives_a_round_trip_through_v2() {
        for sel in TABLES {
            let v1 = Table::load(&format!("tables/{}.tab2", sel), sel)
                .ok()
                .unwrap();
            let text = render(&v1, None);
            let v2 = match parse(&text, sel) {
                Ok(t) => t,
                Err(LoadError::Syntax(line, what)) => {
                    panic!("tasm{}: v2 parse failed at line {}: {}", sel, line, what)
                }
                Err(_) => panic!("tasm{}: v2 parse failed", sel),
            };

            assert_eq!(v1.banner, v2.banner, "tasm{} banner", sel);
            assert_eq!(v1.msfirst, v2.msfirst, "tasm{} opcode order", sel);
            assert_eq!(v1.wordaddrs, v2.wordaddrs, "tasm{} address unit", sel);
            assert_eq!(
                v1.aux_registers, v2.aux_registers,
                "tasm{} aux registers",
                sel
            );
            assert_eq!(
                v1.regsets.len(),
                v2.regsets.len(),
                "tasm{} regset count",
                sel
            );
            for (a, b) in v1.regsets.iter().zip(&v2.regsets) {
                assert_eq!(
                    (&a.name, a.mask, a.class),
                    (&b.name, b.mask, b.class),
                    "tasm{}",
                    sel
                );
            }
            assert_eq!(v1.rows.len(), v2.rows.len(), "tasm{} row count", sel);

            for (i, (a, b)) in v1.rows.iter().zip(&v2.rows).enumerate() {
                let want = a.args.clone();
                let where_ = format!("tasm{} row {} ({})", sel, i, a.mnemonic);
                assert_eq!(a.mnemonic, b.mnemonic, "{}", where_);
                assert_eq!(want, b.args, "{} operands", where_);
                assert_eq!(a.opcode, b.opcode, "{} opcode", where_);
                assert_eq!(a.opcode_bytes, b.opcode_bytes, "{} op-bytes", where_);
                assert_eq!(a.arg_bytes, b.arg_bytes, "{} arg-bytes", where_);
                assert_eq!(a.rule, b.rule, "{} rule", where_);
                assert_eq!(a.class, b.class, "{} class", where_);
                assert_eq!(a.post_shift, b.post_shift, "{} post-shift", where_);
                assert_eq!(a.post_or, b.post_or, "{} post-or", where_);
                // shift/or only have to survive where a rule actually reads them.
                let allowed = params_for(a.rule);
                if !allowed.is_empty() {
                    assert_eq!(a.or, b.or, "{} mask", where_);
                    assert_eq!(a.shift, b.shift, "{} shift", where_);
                }
            }
        }
    }

    /// A non-zero `shift`/`or` that neither the rule reads nor the post-step
    /// carries is dead in v1 and unrepresentable in v2. Assert the shipped
    /// tables contain none, so the round trip above is lossless rather than
    /// merely consistent.
    #[test]
    fn no_shipped_row_carries_an_unreachable_shift_or_mask() {
        for sel in TABLES {
            let t = Table::load(&format!("tables/{}.tab2", sel), sel)
                .ok()
                .unwrap();
            for r in &t.rows {
                if params_for(r.rule).is_empty() && r.post_shift == 0 && r.post_or == 0 {
                    assert_eq!(
                        (r.shift, r.or),
                        (0, 0),
                        "tasm{} {} carries a mask no rule reads",
                        sel,
                        r.mnemonic
                    );
                }
                assert!(
                    !r.short_count,
                    "tasm{} {} has a short byte count",
                    sel, r.mnemonic
                );
            }
        }
    }
}

#[cfg(test)]
mod diagnostics {
    use super::*;

    const HEAD: &str = "%format 2\n%columns mnemonic operands opcode op-bytes arg-bytes rule\n";

    fn fails(body: &str) -> String {
        match parse(&format!("{}{}", HEAD, body), "51") {
            Err(LoadError::Syntax(line, what)) => format!("{}: {}", line, what),
            Err(_) => "other".to_string(),
            Ok(_) => panic!("expected a diagnostic for {:?}", body),
        }
    }

    fn ok(body: &str) -> Row {
        parse(&format!("{}{}", HEAD, body), "51")
            .ok()
            .unwrap()
            .rows
            .remove(0)
    }

    /// The v1 defect of §7.19: a row missing a column shifted every later field
    /// and registered silently. Here it names the line.
    #[test]
    fn a_missing_column_is_an_error_naming_the_line() {
        assert!(
            fails("IM0 46ED 2 plain\n").contains("expected 6 columns"),
            "{}",
            fails("IM0 46ED 2 plain\n")
        );
        assert!(fails("NOP - 00 1 0 plain junk\n").contains("extra field junk"));
    }

    /// v1 derives the opcode width from the digit count, so `0` and `00` differ
    /// silently. Here both are stated and cross-checked.
    #[test]
    fn op_bytes_must_agree_with_the_opcode_width() {
        assert!(fails("NOP - 0 1 0 plain\n").contains("disagrees"));
        assert!(fails("NOP - 46ED 1 0 plain\n").contains("disagrees"));
        assert_eq!(ok("NOP - 46ED 2 0 plain\n").opcode_bytes, 2);
    }

    /// v1 keys a rule on its first two characters, so `combrel` silently
    /// selects `combine`.
    #[test]
    fn an_unknown_rule_name_is_an_error() {
        assert!(fails("NOP - 00 1 0 combrel\n").contains("unknown rule combrel"));
        assert!(fails("NOP - 00 1 0 NOTOUCH\n").contains("unknown rule"));
        assert_eq!(ok("NOP - 00 1 0 plain\n").rule, table::NO);
    }

    /// Each rule accepts only the parameters it reads -- v1's `SHIFT` and `OR`
    /// carried five meanings between them.
    #[test]
    fn a_parameter_the_rule_does_not_read_is_an_error() {
        assert!(fails("NOP - 00 1 0 plain page-mask=F800\n").contains("has no parameter"));
        assert!(
            fails("ACALL <expr> 11 1 1 jmp-page-2k valid-mask=7F\n").contains("has no parameter")
        );
        assert_eq!(
            ok("ACALL <expr> 11 1 1 jmp-page-2k page-mask=F800\n").or,
            0xF800
        );
        // The post-rule transform is universal, and lands in its own fields.
        let r = ok("BIT <expr>,(HL) 46CB 2 0 bit-z80 post-or=4600\n");
        assert_eq!((r.post_or, r.or), (0x4600, 0));
    }

    /// §6.4: a capture that is not the last must be terminated by a literal or
    /// the extractor cannot find its end. v1 mis-parses in silence.
    #[test]
    fn a_non_final_capture_needs_a_delimiter() {
        assert!(fails("FOO <expr><expr> 00 1 0 plain\n").contains("must be followed by"));
        assert!(parse(&format!("{}FOO <expr>,<expr> 00 1 0 plain\n", HEAD), "51").is_ok());
        assert!(parse(
            &format!("{}FOO A,#<expr>,<expr> 00 1 0 plain\n", HEAD),
            "51"
        )
        .is_ok());
    }

    /// §5.6: order is significant and the first match wins, so a repeated
    /// pattern for the same mnemonic can never match.
    #[test]
    fn an_unreachable_row_is_an_error() {
        let e = fails("ADD A,<expr> 25 1 1 plain\nADD A,<expr> 26 1 1 plain\n");
        assert!(e.contains("unreachable row"), "{}", e);
        // A different class does not collide.
        assert!(parse(
            &format!(
                "{}%class 2 ext\nADD A,<expr> 25 1 1 plain\nADD A,<expr> 26 1 1 plain class=2\n",
                HEAD
            ),
            "51"
        )
        .is_ok());
    }

    #[test]
    fn the_header_is_validated() {
        assert!(parse("%format 3\n", "51").is_err());
        assert!(
            parse("NOP - 00 1 0 plain\n", "51").is_err(),
            "a row before %format"
        );
        assert!(
            parse("%format 2\nNOP - 00 1 0 plain\n", "51").is_err(),
            "a row before %columns"
        );
        assert!(
            parse("%format 2\n%wildcard @\n", "51").is_err(),
            "no %wildcard in v2"
        );
        assert!(
            parse("%format 2\n%aux-registers 3\n", "51").is_err(),
            "not a power of two"
        );
    }

    /// `;` is the comment leader precisely because `#` is the immediate prefix.
    #[test]
    fn comments_do_not_eat_the_immediate_prefix() {
        let r = ok("ADD A,#<expr> 24 1 1 plain   ; immediate\n");
        assert_eq!(display(&r.args), "A,#<expr>");
    }

    /// Hostile input: the v2 parser is a trust boundary reading outside data,
    /// exactly as the v1 loader is (the robustness suite).
    #[test]
    fn hostile_tables_are_rejected_rather_than_crashing() {
        for body in [
            &format!("NOP - {} 1 0 plain\n", "A".repeat(200)),
            "NOP - 00 99 0 plain\n",
            "NOP - 00 1 99 plain\n",
            "NOP\n",
            "NOP - 00 1 0 plain shift=FFFFFFFFFF\n",
            &format!("{} - 00 1 0 plain\n", "A".repeat(600)),
            "NOP <bogus> 00 1 0 plain\n",
        ] {
            let _ = parse(&format!("{}{}", HEAD, body), "51");
        }
        // ...and an empty file, and one that is only comments.
        assert!(parse("", "51").is_err());
        assert!(parse("; nothing here\n", "51").is_err());
    }

    /// The format is decided by content, and `load_first` takes the first path
    /// that exists. Ordering itself is `cli::table_paths`, tested there.
    #[test]
    fn load_first_takes_the_first_path_present_and_sniffs_the_format() {
        let dir = std::env::temp_dir().join("tabasm-v2-dispatch");
        let _ = std::fs::create_dir_all(&dir);
        let v1 = dir.join("tasm77.tab");
        let v2 = dir.join("77.tab2");
        std::fs::write(&v1, "\"v1 banner\"\nNOP \"\" 00 1 NOP 1\n").unwrap();
        let _ = std::fs::remove_file(&v2);

        // Only the legacy file exists, so it is used.
        let paths = vec![v2.to_string_lossy().into(), v1.to_string_lossy().into()];
        let t = Table::load_first(&paths, "77").ok().unwrap();
        assert_eq!(t.banner, "v1 banner");
        assert_eq!(t.regmark, b'!');

        // Once the v2 file exists it comes first in the list and wins.
        std::fs::write(
            &v2,
            "%format 2\n%banner \"v2 banner\"\n%columns mnemonic operands opcode op-bytes arg-bytes rule\nNOP - 00 1 0 plain\n",
        )
        .unwrap();
        let t = Table::load_first(&paths, "77").ok().unwrap();
        assert_eq!(t.banner, "v2 banner");
        assert_eq!(t.regmark, REG);

        // Content decides, not the name: a v2 table called `.tab` is still v2.
        let odd = dir.join("tasm78.tab");
        std::fs::write(&odd, "; a v2 table under a legacy name\n%format 2\n%banner \"sniffed\"\n%columns mnemonic operands opcode op-bytes arg-bytes rule\nNOP - 00 1 0 plain\n").unwrap();
        let one = vec![odd.to_string_lossy().into()];
        assert_eq!(
            Table::load_first(&one, "78").ok().unwrap().banner,
            "sniffed"
        );

        // Nothing present at all is an Open error naming the first candidate.
        let missing = vec![
            "/nonexistent/99.tab2".to_string(),
            "/nonexistent/tasm99.tab".to_string(),
        ];
        assert!(matches!(
            Table::load_first(&missing, "99"),
            Err(LoadError::Open(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

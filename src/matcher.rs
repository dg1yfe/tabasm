//! Matching a source line against the instruction table.
//!
//! Rows are tried in table order, and the first row whose mnemonic and operand
//! pattern both match wins -- so a table orders its rows from most specific to
//! least. The class mask carries the -x style switches, which enable or
//! suppress whole groups of rows.

use crate::limits::MAX_ARGS;
use crate::table::{Row, Table};

pub struct Matched {
    pub row: usize,
    /// The row's opcode with any register-set mask folded in.
    pub opcode: u32,
    /// Captured operand expressions, in ORIGINAL case: labels are
    /// case-sensitive by default, so the upper-cased copy is only ever used
    /// for comparison (6.1).
    pub args: Vec<String>,
}

/// 6.2: the original indexes rows by a hash of the mnemonic's first two
/// characters, then scans linearly forward to the end of the table and never
/// wraps. Because it never wraps, a plain scan from row 0 gives identical
/// results -- the hash is an optimisation, not a semantic -- so this is the
/// simple form.
///
/// 6.3/5.6: the FIRST row whose mnemonic, class and operand pattern all match
/// wins, and scanning stops there. Row order is the language the table accepts.
pub fn find(table: &Table, mnemonic: &str, operand: &str, class_mask: u32) -> Option<Matched> {
    let mnem = mnemonic.to_ascii_uppercase();
    // 6.1: the matcher works on operand text with whitespace removed -- but
    // not inside a quoted character or string, where a space is the value
    // itself. `ldab #' '` loads 0x20, not the quote that closing up the gap
    // would leave behind.
    let mut quote: Option<u8> = None;
    let orig: Vec<u8> = operand
        .bytes()
        .filter(|b| {
            match quote {
                Some(q) => {
                    if *b == q {
                        quote = None;
                    }
                    return true;
                }
                None => {
                    if *b == b'\'' || *b == b'"' {
                        quote = Some(*b);
                        return true;
                    }
                }
            }
            !b.is_ascii_whitespace()
        })
        .collect();
    let upper: Vec<u8> = orig.iter().map(|b| b.to_ascii_uppercase()).collect();

    for (i, row) in table.rows.iter().enumerate() {
        if row.mnemonic != mnem {
            continue;
        }
        if row.class & class_mask == 0 {
            continue;
        }
        if let Some(m) = walk(table, row, i, &orig, &upper, class_mask) {
            return Some(m);
        }
    }
    None
}

/// 6.4: two cursors advance together, one over the row's ARGS pattern and one
/// over the operand text. 6.5: the match succeeds only when both reach their
/// ends simultaneously.
fn walk(
    table: &Table,
    row: &Row,
    index: usize,
    orig: &[u8],
    upper: &[u8],
    class_mask: u32,
) -> Option<Matched> {
    // 5.4: `""` is the pattern for an instruction that takes no operands.
    if row.args == "\"\"" {
        return if orig.is_empty() {
            Some(Matched {
                row: index,
                opcode: row.opcode,
                args: Vec::new(),
            })
        } else {
            None
        };
    }

    let pat = row.args.as_bytes();
    let wild = table.wildcard as u8;
    let (mut p, mut o) = (0usize, 0usize);
    let mut args: Vec<String> = Vec::new();
    // 6.4: a register mask is ASSIGNED, not OR-accumulated, so with more than
    // one `!` slot the last match wins. No shipped table has two, which is why
    // 5.7 and 6.5 can describe it loosely as "OR-ed into the opcode".
    let mut reg_mask: Option<u32> = None;

    while p < pat.len() {
        let c = pat[p];

        if c == wild {
            let tail = &pat[p + 1..];
            let end = match tail.first() {
                // 5.4: every wildcard except the last is followed by ',', '['
                // or ']', and the extractor uses that delimiter to terminate
                // the capture.
                Some(&d @ (b',' | b'[' | b']')) => {
                    let k = upper[o..].iter().position(|&b| b == d)?;
                    o + k
                }
                // Patterns like `*),` end the capture at the `),`.
                Some(b')') if tail.get(1) == Some(&b',') => {
                    let k = upper[o..].windows(2).position(|w| w == b"),")?;
                    o + k
                }
                // The last wildcard: leave as many trailing operand
                // characters as there are pattern characters after it.
                _ => {
                    if orig.len() < o + tail.len() {
                        return None;
                    }
                    orig.len() - tail.len()
                }
            };
            // 6.4: a pattern with more than 128 wildcards (MAXARGS) is
            // rejected -- the row cannot capture that many operands.
            if args.len() >= MAX_ARGS {
                return None;
            }
            args.push(String::from_utf8_lossy(&orig[o..end]).into_owned());
            o = end;
            p += 1;
            continue;
        }

        if c == table.regmark {
            // 6.4: scan the register table IN ORDER for an entry whose name is
            // a prefix of the remaining operand text. Declaration order is
            // load-bearing, because `*BR0+` must be tried before `*0+`.
            let mut hit = false;
            for rs in &table.regsets {
                if rs.class & class_mask == 0 {
                    continue;
                }
                let n = rs.name.as_bytes();
                if upper.len() >= o + n.len()
                    && upper[o..o + n.len()]
                        .iter()
                        .zip(n)
                        .all(|(a, b)| *a == b.to_ascii_uppercase())
                {
                    reg_mask = Some(rs.mask);
                    o += n.len();
                    hit = true;
                    break;
                }
            }
            if !hit {
                return None;
            }
            p += 1;
            continue;
        }

        // A literal. 6.1: comparison is case-insensitive.
        if o >= upper.len() || upper[o] != c.to_ascii_uppercase() {
            return None;
        }
        o += 1;
        p += 1;
    }

    if o != orig.len() {
        return None; // left-over operand text rejects the row (6.5)
    }
    Some(Matched {
        row: index,
        opcode: row.opcode | reg_mask.unwrap_or(0),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{self, Table};

    fn load(sel: &str) -> Table {
        Table::load(&format!("tables/{}.tab2", sel), sel)
            .ok()
            .unwrap()
    }

    #[test]
    fn worked_example_from_6_6() {
        let t = load("8051");
        let m = find(&t, "CJNE", "A,#25h,loop", 1).expect("CJNE must match");
        let row = &t.rows[m.row];
        assert_eq!(crate::table2::display(&row.args), "A,#<expr>,<expr>");
        assert_eq!((m.opcode, row.opcode_bytes, row.arg_bytes), (0xB4, 1, 2));
        assert_eq!(row.rule, table::CR);
        // The capture keeps the source's own case: labels are case-sensitive.
        assert_eq!(m.args, vec!["25h".to_string(), "loop".to_string()]);
    }

    #[test]
    fn row_order_decides_which_form_wins() {
        // 5.6: `ADD A,#*` precedes `ADD A,*`, so an immediate operand reaches
        // the immediate row. Nothing prefers a more specific pattern -- swap
        // the rows and `A,*` would capture "#5" instead.
        let t = load("8051");
        let m = find(&t, "ADD", "A,#5", 1).unwrap();
        assert_eq!(crate::table2::display(&t.rows[m.row].args), "A,#<expr>");
        assert_eq!(m.args, vec!["5".to_string()]);
        let m = find(&t, "ADD", "A,25h", 1).unwrap();
        assert_eq!(crate::table2::display(&t.rows[m.row].args), "A,<expr>");
    }

    #[test]
    fn empty_operand_pattern_requires_empty_operands() {
        let t = load("8051");
        assert!(find(&t, "NOP", "", 1).is_some());
        assert!(find(&t, "NOP", "A", 1).is_none());
        // The repaired Z80 rows, which is what z80-im-alias pins.
        let t = load("z80");
        for (m, op) in [("IM0", 0x46EDu32), ("IM1", 0x56ED), ("IM2", 0x5EED)] {
            assert_eq!(find(&t, m, "", 1).unwrap().opcode, op);
        }
        // The spaced spelling reaches the same encodings.
        for (a, op) in [("0", 0x46EDu32), ("1", 0x56ED), ("2", 0x5EED)] {
            assert_eq!(find(&t, "IM", a, 1).unwrap().opcode, op);
        }
    }

    #[test]
    fn matching_is_case_insensitive_but_capture_is_not() {
        let t = load("8051");
        let m = find(&t, "mov", "A,#MixedCase", 1).unwrap();
        assert_eq!(m.args, vec!["MixedCase".to_string()]);
    }

    #[test]
    fn whitespace_in_operands_is_removed_before_matching() {
        let t = load("8051");
        assert!(find(&t, "CJNE", "A , # 25h , loop", 1).is_some());
    }

    #[test]
    fn class_mask_gates_rows() {
        // The 8048 table holds 8021/8022/8041 extensions on class bits 2, 4, 8.
        let t = load("8048");
        assert!(find(&t, "EN", "DMA", 1).is_none(), "class 2 needs -x2");
        assert!(find(&t, "EN", "DMA", 2).is_some());
    }

    #[test]
    fn register_sets_match_by_prefix_in_declaration_order() {
        // The C25 table declares *BR0+ and *0+; the longest name wins, so a
        // prefix match cannot stop at the shorter one.
        let t = load("tms320c25");
        let m = find(&t, "ADD", "*BR0+,4", 1);
        assert!(m.is_some(), "*BR0+ must match its own regset");
        let m = m.unwrap();
        // 0xF0 is the *BR0+ mask; the base opcode carries 0x80 already.
        assert_eq!(m.opcode & 0xF0, 0xF0);
    }

    #[test]
    fn a_slash_in_the_pattern_is_a_literal() {
        // `ANL C,/<expr>` -- '/' is the 8051 complement-bit operator.
        let t = load("8051");
        let m = find(&t, "ANL", "C,/40h", 1).expect("ANL C,/expr must match");
        assert_eq!(t.rows[m.row].opcode, 0xb0);
        assert_eq!(m.args, vec!["40h".to_string()]);
    }

    #[test]
    fn an_unknown_mnemonic_matches_nothing() {
        let t = load("8051");
        assert!(find(&t, "FROBNICATE", "a,r0", 1).is_none());
        assert!(find(&t, "WIBBLE", "", 1).is_none());
    }
}

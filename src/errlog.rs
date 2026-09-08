//! Diagnostics (9.5, 10.3).
//!
//! 2.6: diagnostics go to standard output AND into the listing, never to
//! standard error. Standard error carries only the debug trace and warnings
//! about the environment -- of which the TASMERRFORMAT rejection below is one,
//! and it IS compared (tests/golden/errfmt-mismatch.err).

/// The exact message strings. Capitalisation and trailing spaces are part of
/// the text: three of these are padded to 34 columns and the rest are not,
/// and several exist in both an upper- and a lower-case form because the
/// original raised them from different sites.
pub mod msg {
    // Symbols
    pub const LABEL_NOT_FOUND: &str = "Label not found:";
    pub const NO_SUCH_LABEL: &str = "No such label:";
    pub const DUPLICATE_LABEL: &str = "Duplicate label:";
    pub const LABEL_TOO_LONG: &str = "Label too long:";
    pub const TOKEN_TOO_LONG: &str = "Label or instruction too long.";
    pub const MISALIGNED: &str = "label value misalligned.          ";
    pub const SET_PREEXIST: &str = "Label must pre-exist for SET:";
    pub const FORWARD_IN_EQUATE: &str = "Forward reference in equate:";
    pub const LABEL_TABLE_OVERFLOW: &str = "label table overflow              ";

    // Expressions
    pub const BINOP_WHERE_VALUE: &str = "Binary operator where a value expected:";
    pub const INVALID_TOKEN: &str = "Invalid token where value expected:";
    pub const NON_UNARY: &str = "Non-unary operator at beginning of expression.";
    pub const PAREN_IMBALANCE: &str = "Paren imbalance.";
    pub const NO_TERMINATING_QUOTE: &str = "No terminating quote:";
    pub const DIVIDE_BY_ZERO: &str = "Division by zero.";
    pub const MODULO_BY_ZERO: &str = "Modulo by zero.";
    pub const NEGATIVE_SHIFT: &str = "Negative shift count.";

    // Instructions and operands
    pub const BAD_DIRECTIVE: &str = "unrecognized directive.           ";
    pub const BAD_INSTRUCTION: &str = "unrecognized instruction.         ";
    pub const BAD_ARGUMENT: &str = "unrecognized argument.            ";
    pub const RANGE_ARG: &str = "Range of argument exceeded.";
    pub const RANGE_ARG_LC: &str = "range of argument exceeded.";
    pub const RANGE_BRANCH: &str = "Range of relative branch exceeded.";
    pub const RANGE_BRANCH_LC: &str = "range of relative branch exceeded.";
    pub const OFF_2K_PAGE: &str = "Branch off of current 2K page.";
    pub const OFF_PAGE: &str = "Branch off of current page.";
    pub const UNUSED_MS_BYTE: &str = "Unused data in MS byte of argument.";
    pub const INVALID_MODOP: &str = "Invalid MODOP.";
    /// Two spaces after the first '.', per 10.3 and 1.4.
    pub const NO_INDIRECTION: &str = "Invalid operand.  No indirection for this instruction.";
    pub const RANGE_ARP: &str = "Range of ARP argument exceeded.";
    pub const OUTSIDE_IMAGE: &str = "Address outside memory image.";
    pub const SHORT_BYTE_COUNT: &str = "Table entry byte count is less than the opcode size.";

    // Directives and structure
    pub const NO_END: &str = "No END directive before EOF.      ";
    pub const IMBALANCED_COND: &str = "Imbalanced conditional.";
    pub const ENDIF_NO_MATCH: &str = "ENDIF without a matching conditional.";
    pub const ELSE_NO_MATCH: &str = "ELSE without a matching conditional.";
    pub const COND_TOO_DEEP: &str = "Max number of nested conditionals exceeded.";
    pub const INCLUDE_TOO_DEEP: &str = "Include nesting too deep.";
    pub const END_OUT_OF_RANGE: &str = "END address out of range.";
    pub const TOO_MANY_ARGS: &str = "Maximum number of args exceeded.";

    // Macros
    pub const MACRO_TOO_LONG: &str = "Macro expansion too long         ";
    pub const MACRO_TOO_DEEP: &str = "Macro expansion too deep (recursive macro?).";
    pub const DEFCONT_NO_DEFINE: &str = "DEFCONT with no preceding DEFINE.";
    pub const MACRO_EXPECTS_ARGS: &str = "Macro expects args but none found";
    pub const TOO_MANY_MACROS: &str = "Maximum number of macros exceeded ";
}

/// A validated TASMERRFORMAT, or the built-in layout.
///
/// 9.5: the format is applied to exactly four arguments, in order: the source
/// file name (string), the line number (integer), the message (string), and
/// the parenthesised detail or an empty string. A format whose conversions do
/// not match those types in that order must be REJECTED rather than passed to
/// a formatting function -- unvalidated, this was an exploitable format-string
/// vulnerability, and `%s %n` a write primitive.
pub struct Format {
    spec: Option<String>,
}

/// 9.5: a huge width is ACCEPTED, not rejected -- it is a conforming
/// conversion. The message is built with a bounded write into the line buffer,
/// so an enormous width is padded and then truncated at the buffer rather than
/// overflowing it.
///
/// The cut-off is measured from the `errfmt-bigwidth` vector, whose rendered
/// line is exactly 510 characters -- two short of LINE_SIZE, the room the
/// original's buffer leaves for the newline and terminator.
const RENDER_LIMIT: usize = LINE_SIZE - 2;

use crate::limits::LINE_SIZE;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Conv {
    Str,
    Int,
}

impl Format {
    pub fn default_layout() -> Format {
        Format { spec: None }
    }

    /// Returns the format and, when the environment variable was rejected,
    /// the warning to print on standard error.
    pub fn from_env(spec: Option<&str>) -> (Format, Option<&'static str>) {
        let spec = match spec {
            Some(s) => s,
            None => return (Format::default_layout(), None),
        };
        match parse(spec) {
            Some(convs) if convs == [Conv::Str, Conv::Int, Conv::Str, Conv::Str][..convs.len()] => {
                (Format { spec: Some(spec.to_string()) }, None)
            }
            _ => (
                Format::default_layout(),
                Some("ignoring TASMERRFORMAT: unsupported conversion (expected %s, %d, %s, %s in that order)"),
            ),
        }
    }

    pub fn render(&self, file: &str, line: u32, message: &str, detail: Option<&str>) -> String {
        let detail = match detail {
            Some(d) => format!("({})", d),
            None => String::new(),
        };
        match &self.spec {
            // 9.5: the built-in layout is equivalent to "%s line %04d: %s %s",
            // which is why a message with no detail still ends in a space.
            None => format!("{} line {:04}: {} {}", file, line, message, detail),
            Some(spec) => apply(spec, file, line, message, &detail),
        }
    }
}

/// Scan a format and return the conversions it uses, or None if it uses one we
/// will not support. `%%` is a literal and contributes nothing.
fn parse(spec: &str) -> Option<Vec<Conv>> {
    let b = spec.as_bytes();
    let mut convs = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if i < b.len() && b[i] == b'%' {
            i += 1;
            continue;
        }
        while i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'#' | b'0') {
            i += 1;
        }
        // A '*' takes the width from the argument list, which we do not
        // supply. Reject rather than guess.
        if i < b.len() && b[i] == b'*' {
            return None;
        }
        // A width of any size is conforming; it is bounded at render time.
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        match b.get(i) {
            Some(b's') => convs.push(Conv::Str),
            Some(b'd') | Some(b'i') => convs.push(Conv::Int),
            _ => return None, // %n, %x, %f, a bare trailing '%', ...
        }
        i += 1;
        if convs.len() > 4 {
            return None;
        }
    }
    Some(convs)
}

/// Apply an already-validated format. Only the subset `parse` accepts is
/// reachable here.
fn apply(spec: &str, file: &str, line: u32, message: &str, detail: &str) -> String {
    let b = spec.as_bytes();
    let mut out = String::new();
    let mut argi = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'%' {
            out.push(b[i] as char);
            i += 1;
            continue;
        }
        i += 1;
        if i < b.len() && b[i] == b'%' {
            out.push('%');
            i += 1;
            continue;
        }
        let (mut left, mut zero) = (false, false);
        while i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'#' | b'0') {
            match b[i] {
                b'-' => left = true,
                b'0' => zero = true,
                _ => {}
            }
            i += 1;
        }
        let ws = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        // An absent width is zero, not unbounded.
        let width: usize = if i > ws { spec[ws..i].parse().unwrap_or(RENDER_LIMIT) } else { 0 };
        let mut prec: Option<usize> = None;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            let ps = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            prec = Some(spec[ps..i].parse::<usize>().unwrap_or(RENDER_LIMIT));
        }
        let conv = b[i];
        i += 1;

        let mut s = match (conv, argi) {
            (b's', 0) => file.to_string(),
            (b's', 2) => message.to_string(),
            (b's', _) => detail.to_string(),
            (_, _) => {
                if zero {
                    format!("{:0w$}", line, w = width)
                } else {
                    line.to_string()
                }
            }
        };
        argi += 1;
        if conv == b's' {
            if let Some(p) = prec {
                s.truncate(p);
            }
        }
        if s.len() < width {
            // Pad only as far as the buffer can hold: `%99999999s` must not
            // allocate 100 MB on the way to being truncated.
            let room = RENDER_LIMIT.saturating_sub(out.len());
            let pad = " ".repeat((width - s.len()).min(room));
            if left {
                s.push_str(&pad);
            } else {
                s.insert_str(0, &pad);
            }
        }
        out.push_str(&s);
        if out.len() >= RENDER_LIMIT {
            out.truncate(RENDER_LIMIT);
            return out;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact lines from tests/golden/err-undef.out and friends.
    #[test]
    fn default_layout_matches_the_corpus() {
        let f = Format::default_layout();
        assert_eq!(
            f.render("err-undef.asm", 4, msg::LABEL_NOT_FOUND, Some("no_such_label")),
            "err-undef.asm line 0004: Label not found: (no_such_label)"
        );
        // A message with no detail still ends in the format's space.
        assert_eq!(
            f.render("err-range.asm", 5, msg::RANGE_BRANCH, None),
            "err-range.asm line 0005: Range of relative branch exceeded. "
        );
        // Three messages carry trailing padding to 34 columns.
        assert_eq!(
            f.render("err-baddir.asm", 4, msg::BAD_DIRECTIVE, Some(".NOTADIRECTIVE")),
            "err-baddir.asm line 0004: unrecognized directive.            (.NOTADIRECTIVE)"
        );
        assert_eq!(
            f.render("err-badinst.asm", 4, msg::BAD_INSTRUCTION, Some("FROBNICATE")),
            "err-badinst.asm line 0004: unrecognized instruction.          (FROBNICATE)"
        );
        assert_eq!(
            f.render("zp-phase.asm", 23, msg::MISALIGNED, Some("after")),
            "zp-phase.asm line 0023: label value misalligned.           (after)"
        );
    }

    #[test]
    fn accepted_custom_formats_match_the_corpus() {
        for (spec, want) in [
            ("%s(%d): %s %s", "err-undef.asm(4): Label not found: (no_such_label)"),
            ("%s", "err-undef.asm"),
            ("%-20s %04d %s%s", "err-undef.asm        0004 Label not found:(no_such_label)"),
        ] {
            let (f, warn) = Format::from_env(Some(spec));
            assert!(warn.is_none(), "{} should be accepted", spec);
            assert_eq!(
                f.render("err-undef.asm", 4, msg::LABEL_NOT_FOUND, Some("no_such_label")),
                want,
                "format {}",
                spec
            );
        }
    }

    #[test]
    fn mismatched_formats_are_rejected_and_fall_back() {
        // errfmt-mismatch applies %s to the integer; errfmt-percent-n is a
        // write primitive. Both golden .out files show the DEFAULT layout,
        // and both golden .err files carry the warning.
        for spec in ["%s %s %s %s", "%s %n", "%d %d %d %d", "%*s"] {
            let (f, warn) = Format::from_env(Some(spec));
            assert!(warn.is_some(), "{} must be rejected", spec);
            assert_eq!(
                f.render("err-undef.asm", 4, msg::LABEL_NOT_FOUND, Some("no_such_label")),
                "err-undef.asm line 0004: Label not found: (no_such_label)"
            );
        }
    }

    /// 9.5: a huge width is a CONFORMING conversion, so it is honoured rather
    /// than rejected -- padded, then truncated at the line buffer. The
    /// errfmt-bigwidth vector pins the result at 510 characters.
    #[test]
    fn a_huge_field_width_is_honoured_and_truncated_not_rejected() {
        let (f, warn) = Format::from_env(Some("%99999999s"));
        assert!(warn.is_none(), "a large width is conforming");
        let out = f.render("err-undef.asm", 4, msg::LABEL_NOT_FOUND, Some("no_such_label"));
        assert_eq!(out.len(), 510);
        assert!(out.chars().all(|c| c == ' '), "the name never reaches the buffer");
    }

    #[test]
    fn the_rejection_warning_matches_the_golden_err_files() {
        let (_, warn) = Format::from_env(Some("%s %n"));
        assert_eq!(
            format!("tasm: {}", warn.unwrap()),
            "tasm: ignoring TASMERRFORMAT: unsupported conversion (expected %s, %d, %s, %s in that order)"
        );
    }
}

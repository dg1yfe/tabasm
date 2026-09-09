//! Macros: textual substitution, not symbols.
//!
//! A macro is expanded by rewriting the source line before it is matched, so a
//! macro can supply a mnemonic, an operand or any fragment of either. Because
//! expansion can produce further macro calls, it repeats to a bounded depth.

use crate::limits::*;

/// 4.7 caps the NUMBER OF PASSES as well as the length of one expansion, and
/// says both are needed: a cyclic macro re-substitutes at unchanged length and
/// so never trips the length cap.
///
/// The cap must SCALE WITH THE MACRO COUNT, because a legitimate acyclic chain
/// can be as deep as there are macros -- a fixed cap would wrongly reject
/// deep-but-finite nesting.
fn max_passes(n_macros: usize) -> usize {
    n_macros + LINE_SIZE
}

#[derive(Clone, Debug)]
pub struct Macro {
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
}

pub struct Macros {
    pub list: Vec<Macro>,
    pub overflowed: bool,
}

impl Macros {
    pub fn new() -> Macros {
        Macros {
            list: Vec::new(),
            overflowed: false,
        }
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.list.iter().position(|m| m.name == name)
    }

    pub fn defined(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// `.DEFINE <name>[(<params>)] <body>`
    pub fn define(&mut self, operand: &str) -> Option<&'static str> {
        let s = operand.trim_start();
        if s.is_empty() {
            return None; // a bare #define is not an error, just nothing
        }
        let end = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(s.len());
        let name = &s[..end];
        if name.is_empty() {
            return None;
        }
        let rest = &s[end..];
        let (params, body) = if rest.starts_with('(') {
            match rest.find(')') {
                Some(close) => {
                    let ps: Vec<String> = rest[1..close]
                        .split(',')
                        .map(|p| p.trim().chars().take(MAX_MACRO_PARAM_LEN).collect())
                        .filter(|p: &String| !p.is_empty())
                        .take(MAX_MACRO_PARAMS)
                        .collect();
                    (ps, rest[close + 1..].trim_start().to_string())
                }
                None => (Vec::new(), rest.trim_start().to_string()),
            }
        } else {
            (Vec::new(), rest.trim_start().to_string())
        };

        if let Some(i) = self.position(name) {
            self.list[i] = Macro {
                name: name.to_string(),
                params,
                body,
            };
            return None;
        }
        // 10.1: overflow here is NOT fatal -- the macro is dropped and the run
        // continues.
        if self.list.len() >= MAX_MACROS {
            self.overflowed = true;
            return Some(crate::errlog::msg::TOO_MANY_MACROS);
        }
        self.list.push(Macro {
            name: name.to_string(),
            params,
            body,
        });
        None
    }

    /// `.DEFCONT <text>` appends to the most recently defined macro.
    pub fn defcont(&mut self, text: &str) -> Option<&'static str> {
        match self.list.last_mut() {
            Some(m) => {
                m.body.push_str(text.trim_start());
                None
            }
            None => Some(crate::errlog::msg::DEFCONT_NO_DEFINE),
        }
    }

    /// 4.7: undefining a name that is not a macro is not an error.
    pub fn undef(&mut self, name: &str) {
        if let Some(i) = self.position(name) {
            self.list.remove(i);
        }
    }

    /// Expand every macro reference in `line`, repeatedly, until a full pass
    /// substitutes nothing.
    pub fn expand(&self, line: &str, comment_char: u8) -> (String, Vec<&'static str>) {
        let mut errs = Vec::new();
        // 4.7: expansion is skipped entirely when the first non-blank
        // character is '#' or the column-1 comment character. This is what
        // protects `#define` from expanding itself -- and what `.define` does
        // NOT get, which is why the shipped headers use the '#' spellings.
        let first = line.trim_start().as_bytes().first().copied();
        if first == Some(b'#') || first == Some(comment_char) {
            return (line.to_string(), errs);
        }
        if self.list.is_empty() {
            return (line.to_string(), errs);
        }

        let mut cur = line.to_string();
        let cap = max_passes(self.list.len());
        for pass in 0..=cap {
            if pass == cap {
                errs.push(crate::errlog::msg::MACRO_TOO_DEEP);
                break;
            }
            let (next, changed, mut e) = self.one_pass(&cur);
            errs.append(&mut e);
            if next.len() >= LINE_SIZE {
                errs.push(crate::errlog::msg::MACRO_TOO_LONG);
                cur = next.chars().take(LINE_SIZE - 1).collect();
                break;
            }
            cur = next;
            if !changed {
                break;
            }
        }
        (cur, errs)
    }

    fn one_pass(&self, line: &str) -> (String, bool, Vec<&'static str>) {
        let b = line.as_bytes();
        let mut out = String::new();
        let mut errs = Vec::new();
        let mut changed = false;
        let mut i = 0usize;
        let mut quote: Option<u8> = None;

        while i < b.len() {
            let c = b[i];
            // 4.7: not inside quotes.
            if let Some(q) = quote {
                out.push(c as char);
                if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            if c == b'"' || c == b'\'' {
                quote = Some(c);
                out.push(c as char);
                i += 1;
                continue;
            }
            if !(c.is_ascii_alphabetic() || c == b'_') {
                out.push(c as char);
                i += 1;
                continue;
            }
            // 4.7: whole identifiers only -- and an identifier preceded by a
            // symbol-continuation character is part of a larger token, not a
            // fresh one. Without this, `#define equ .equ` expands `equ` to
            // `.equ`, then matches the `equ` inside its own result and grows
            // without bound.
            if start_of_ident_blocked(b, i) {
                out.push(c as char);
                i += 1;
                continue;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            let ident = &line[start..i];
            let m = match self.position(ident) {
                Some(k) => &self.list[k],
                None => {
                    out.push_str(ident);
                    continue;
                }
            };
            // 4.7: arguments are supplied only when '(' follows the name
            // immediately.
            let mut args: Vec<String> = Vec::new();
            if b.get(i) == Some(&b'(') {
                if let Some(close) = find_close(b, i) {
                    args = split_top(&line[i + 1..close]);
                    i = close + 1;
                }
            }
            let (text, mut e) = substitute(m, &args);
            errs.append(&mut e);
            out.push_str(&text);
            changed = true;
        }
        (out, changed, errs)
    }
}

fn start_of_ident_blocked(b: &[u8], i: usize) -> bool {
    match i.checked_sub(1).and_then(|k| b.get(k)) {
        Some(p) => p.is_ascii_alphanumeric() || *p == b'_' || *p == b'.',
        None => false,
    }
}

fn find_close(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, c) in b.iter().enumerate().skip(open) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Substitute a macro's parameters into its body.
///
/// 4.7: a parameter is referenced either by its NAME or by the token `?n`,
/// where n is its ZERO-based position. Name substitution is by whole
/// identifier -- the same rule as macro-name expansion -- so a parameter named
/// `A` is left untouched inside `0Ah` or a longer name.
fn substitute(m: &Macro, args: &[String]) -> (String, Vec<&'static str>) {
    let mut errs = Vec::new();
    let mut out = String::new();
    let b = m.body.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'?' {
            let d = b.get(i + 1).copied().unwrap_or(0);
            if d.is_ascii_digit() {
                // 4.7: `?n` is ZERO-based -- ?0 is the first parameter.
                let n = (d - b'0') as usize;
                match args.get(n) {
                    Some(a) => out.push_str(a),
                    None => errs.push(crate::errlog::msg::MACRO_EXPECTS_ARGS),
                }
                i += 2;
                continue;
            }
            errs.push(crate::errlog::msg::MACRO_EXPECTS_ARGS);
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            // Whole identifiers only, by the same boundary rule macro-name
            // expansion uses: a parameter named `A` must be left untouched
            // inside `0Ah` -- and inside `0A`.
            if start_of_ident_blocked(b, i) {
                out.push(c as char);
                i += 1;
                continue;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &m.body[start..i];
            match m.params.iter().position(|p| p == word) {
                Some(k) => match args.get(k) {
                    Some(a) => out.push_str(a),
                    None => {
                        errs.push(crate::errlog::msg::MACRO_EXPECTS_ARGS);
                        out.push_str(word);
                    }
                },
                None => out.push_str(word),
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    (out, errs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Macros {
        Macros::new()
    }

    #[test]
    fn simple_substitution_as_the_shipped_headers_use_it() {
        let mut ms = m();
        ms.define("equ .equ");
        ms.define("end .end");
        let (s, e) = ms.expand("n:          equ 20h", b';');
        assert_eq!(s, "n:          .equ 20h");
        assert!(e.is_empty());
    }

    #[test]
    fn a_leading_hash_suppresses_expansion_for_the_line() {
        // 4.7: this is what makes `#undef FOO` work, since otherwise FOO would
        // be replaced by its own definition before the directive saw it.
        let mut ms = m();
        ms.define("FOO 1");
        assert_eq!(ms.expand("#undef FOO", b';').0, "#undef FOO");
        assert_eq!(ms.expand("#ifdef FOO", b';').0, "#ifdef FOO");
        // ...and .undef does NOT get that protection.
        assert_eq!(ms.expand(".undef FOO", b';').0, ".undef 1");
    }

    #[test]
    fn whole_identifiers_only_and_not_inside_quotes() {
        let mut ms = m();
        ms.define("FOO 1");
        assert_eq!(
            ms.expand("        .byte FOOBAR", b';').0,
            "        .byte FOOBAR"
        );
        assert_eq!(
            ms.expand("        .text \"FOO\"", b';').0,
            "        .text \"FOO\""
        );
        assert_eq!(ms.expand("        .byte FOO", b';').0, "        .byte 1");
    }

    #[test]
    fn nested_macros_expand_until_nothing_changes() {
        let mut ms = m();
        ms.define("A B");
        ms.define("B C");
        ms.define("C 42");
        assert_eq!(ms.expand("        .byte A", b';').0, "        .byte 42");
    }

    #[test]
    fn cyclic_macros_are_stopped_by_the_pass_cap() {
        // The length cap cannot catch these: they re-substitute at unchanged
        // length. security.sh feeds all three.
        let mut ms = m();
        ms.define("A A");
        let (_, e) = ms.expand("        A", b';');
        assert!(e.contains(&crate::errlog::msg::MACRO_TOO_DEEP));

        let mut ms = m();
        ms.define("A B");
        ms.define("B A");
        let (_, e) = ms.expand("        A", b';');
        assert!(e.contains(&crate::errlog::msg::MACRO_TOO_DEEP));

        let mut ms = m();
        ms.define("F(a) F(a)");
        let (_, e) = ms.expand("        F(9)", b';');
        assert!(e.contains(&crate::errlog::msg::MACRO_TOO_DEEP));
    }

    #[test]
    fn runaway_length_is_capped_independently() {
        let mut ms = m();
        ms.define(&format!("BIG {}", "A".repeat(400)));
        ms.defcont(&"B".repeat(400));
        let (s, _) = ms.expand("        .byte BIG", b';');
        assert!(s.len() < LINE_SIZE);
    }

    #[test]
    fn undef_removes_and_is_silent_on_an_unknown_name() {
        let mut ms = m();
        ms.define("FOO 1");
        ms.define("BAR 2");
        ms.undef("FOO");
        assert!(!ms.defined("FOO"));
        assert!(ms.defined("BAR"), "removal must not disturb neighbours");
        ms.undef("NEVER_DEFINED"); // not an error
    }

    #[test]
    fn defcont_without_a_define_is_an_error() {
        let mut ms = m();
        assert_eq!(ms.defcont("x"), Some(crate::errlog::msg::DEFCONT_NO_DEFINE));
    }

    #[test]
    fn hostile_definitions_do_not_panic() {
        let mut ms = m();
        ms.define("");
        ms.define("M ?!");
        ms.define("M2 ?");
        let _ = ms.expand("        .byte M", b';');
        let _ = ms.expand("        .byte M2", b';');
    }
}

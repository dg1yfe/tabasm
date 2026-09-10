//! Expression evaluation, in both modes.
//!
//! Values are 32 bits wide, and that width is fixed here explicitly rather
//! than inherited from the host word. It matters: a build that let it follow
//! that let them follow a 64-bit `long` printed a negative label as
//! FFFFFFFFFFFFFFF9 where the original printed FFFFFFF9, and that was the only
//! difference across 363 compared artefacts. Hence i32/u32 throughout, with
//! wrapping arithmetic -- overflow here is defined behaviour, not an error.

use crate::errlog::msg;
use crate::limits::UNDEFINED;

#[derive(Clone, Debug)]
pub struct Diag {
    pub msg: &'static str,
    pub detail: Option<String>,
}

pub struct Outcome {
    pub value: i32,
    pub diags: Vec<Diag>,
    /// Names that did not resolve. The caller decides whether that is worth
    /// reporting: 3.8 says an ordinary forward reference is silent in pass 1
    /// and `Label not found:` in pass 2, but inside an .EQU it is reported on
    /// both passes.
    pub undefined: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Xor,
}

impl Op {
    fn prec(self) -> u8 {
        match self {
            Op::Mul | Op::Div | Op::Mod => 8,
            Op::Add | Op::Sub => 7,
            Op::Shl | Op::Shr => 6, // 3.1: shifts bind LOOSER than + and -
            Op::Lt | Op::Gt | Op::Le | Op::Ge => 5,
            Op::Eq | Op::Ne => 4,
            Op::And => 3,
            Op::Xor => 2,
            Op::Or => 1,
        }
    }
}

pub struct Eval<'a> {
    s: &'a [u8],
    pos: usize,
    pc: i32,
    compat: bool,
    local_char: u8,
    lookup: &'a mut dyn FnMut(&str) -> Option<i32>,
    diags: Vec<Diag>,
    undefined: Vec<String>,
    /// -a bit 0x08 (1.4): report an operator used where a unary one was
    /// expected. Off by default; a bare -a turns it on.
    check_non_unary: bool,
}

/// Evaluate `text`. `pc` is the counter at the start of the current statement
/// (3.5), which is what both `$` and `*` yield.
pub fn eval(
    text: &str,
    pc: i32,
    compat: bool,
    local_char: u8,
    lookup: &mut dyn FnMut(&str) -> Option<i32>,
) -> Outcome {
    eval_strict(text, pc, compat, local_char, 0, lookup)
}

pub fn eval_strict(
    text: &str,
    pc: i32,
    compat: bool,
    local_char: u8,
    strict: u32,
    lookup: &mut dyn FnMut(&str) -> Option<i32>,
) -> Outcome {
    let mut e = Eval {
        s: text.as_bytes(),
        pos: 0,
        pc,
        compat,
        local_char,
        lookup,
        diags: Vec::new(),
        undefined: Vec::new(),
        check_non_unary: strict & 0x08 != 0,
    };
    let value = if compat { e.accumulator() } else { e.expr(1) };
    Outcome {
        value,
        diags: e.diags,
        undefined: e.undefined,
    }
}

impl<'a> Eval<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }
    fn at(&self, n: usize) -> Option<u8> {
        self.s.get(self.pos + n).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
            self.pos += 1;
        }
    }
    /// An expression that opens with something that is not a value opens with
    /// a binary operator. 10.3 gives the wording; 1.4 bit 0x08 gates it.
    fn non_unary_at_start(&mut self) {
        if self.check_non_unary && self.peek().is_some() {
            self.err(msg::NON_UNARY, None);
        }
    }

    fn err(&mut self, msg: &'static str, detail: Option<String>) {
        self.diags.push(Diag { msg, detail });
    }

    // --- default mode: precedence climbing (3.1) ---------------------------

    fn expr(&mut self, min_prec: u8) -> i32 {
        let mut lhs = match self.value() {
            Some(v) => v,
            None => {
                self.non_unary_at_start();
                return 0;
            }
        };
        loop {
            let save = self.pos;
            let (op, len) = match self.peek_binop() {
                Some(x) => x,
                None => break,
            };
            if op.prec() < min_prec {
                break;
            }
            self.pos += len;
            // All binary operators are left-associative, so the right-hand
            // side binds only tighter operators.
            let mark = self.pos;
            match self.value_then(op.prec() + 1) {
                Some(rhs) => lhs = self.apply(op, lhs, rhs),
                None => {
                    // 3.6: an operator with no right-hand value simply ends
                    // the expression at that point.
                    self.pos = save.max(mark.min(save));
                    self.pos = save;
                    break;
                }
            }
        }
        lhs
    }

    /// The right-hand side of a binary operator: a value, then any operators
    /// that bind at least as tightly.
    fn value_then(&mut self, min_prec: u8) -> Option<i32> {
        let save = self.pos;
        let mut lhs = match self.value() {
            Some(v) => v,
            None => {
                self.pos = save;
                return None;
            }
        };
        loop {
            let mark = self.pos;
            let (op, len) = match self.peek_binop() {
                Some(x) => x,
                None => break,
            };
            if op.prec() < min_prec {
                break;
            }
            self.pos += len;
            match self.value_then(op.prec() + 1) {
                Some(rhs) => lhs = self.apply(op, lhs, rhs),
                None => {
                    self.pos = mark;
                    break;
                }
            }
        }
        Some(lhs)
    }

    // --- compatibility mode: the accumulator loop (3.1) ---------------------

    /// No precedence at all: a running value, an operator, exactly one
    /// following value, applied immediately. `1+2*3+4` is `((1+2)*3)+4` = 13.
    /// A leading `-` works only because the accumulator starts at zero.
    ///
    /// The first token is read in VALUE position, which is what makes `%1010`
    /// a binary constant rather than a modulo against the empty accumulator
    /// (3.4). Only if no value can be read there does the leading `+` or `-`
    /// fall through to operator handling.
    fn accumulator(&mut self) -> i32 {
        self.accumulate(false)
    }

    /// A parenthesised sub-expression is its own accumulator run, which is why
    /// `1+2*(3+4)` is 21 in compatibility mode.
    fn accumulator_paren(&mut self) -> i32 {
        self.accumulate(true)
    }

    fn accumulate(&mut self, in_paren: bool) -> i32 {
        let mut acc: i32 = 0;
        let mut first = true;
        loop {
            self.skip_ws();
            match self.peek() {
                None => break,
                Some(b')') if in_paren => break,
                _ => {}
            }
            if first {
                first = false;
                let save = self.pos;
                if let Some(v) = self.value() {
                    acc = v;
                    continue;
                }
                self.pos = save;
                self.non_unary_at_start();
            }
            let save = self.pos;
            if let Some((op, len)) = self.peek_binop() {
                self.pos += len;
                match self.value() {
                    Some(rhs) => acc = self.apply(op, acc, rhs),
                    None => {
                        self.pos = save;
                        break;
                    }
                }
                continue;
            }
            // 3.6: `~` and `!` in operator position REPLACE the accumulator,
            // so `5 ~ 3` yields ~3 rather than 5.
            match self.peek() {
                Some(b'~') | Some(b'!') => {
                    let complement = self.peek() == Some(b'~');
                    self.pos += 1;
                    match self.value() {
                        Some(v) => {
                            acc = if complement {
                                !(v as u32) as i32
                            } else {
                                (v == 0) as i32
                            }
                        }
                        None => break,
                    }
                }
                // 3.6: a value where an operator was expected replaces the
                // accumulator.
                _ => match self.value() {
                    Some(v) => acc = v,
                    None => break,
                },
            }
        }
        acc
    }

    // --- operators ----------------------------------------------------------

    /// Look at the next operator without consuming it. Returns the operator
    /// and its length in bytes.
    fn peek_binop(&mut self) -> Option<(Op, usize)> {
        self.skip_ws();
        let a = self.peek()?;
        let b = self.at(1);
        let two = |op: Op| Some((op, 2usize));
        match (a, b) {
            (b'<', Some(b'<')) => two(Op::Shl),
            (b'>', Some(b'>')) => two(Op::Shr),
            (b'<', Some(b'=')) => two(Op::Le),
            (b'>', Some(b'=')) => two(Op::Ge),
            (b'=', Some(b'=')) => two(Op::Eq),
            (b'!', Some(b'=')) => two(Op::Ne),
            (b'<', _) => Some((Op::Lt, 1)),
            (b'>', _) => Some((Op::Gt, 1)),
            (b'=', _) => Some((Op::Eq, 1)), // 3.3: `=` is a synonym for `==`
            (b'+', _) => Some((Op::Add, 1)),
            (b'-', _) => Some((Op::Sub, 1)),
            (b'/', _) => Some((Op::Div, 1)),
            (b'&', _) => Some((Op::And, 1)),
            (b'|', _) => Some((Op::Or, 1)),
            (b'^', _) => Some((Op::Xor, 1)),
            // 3.4: `%` is modulo only where a value does NOT follow as a
            // binary radix prefix -- i.e. here, in operator position.
            (b'%', _) => Some((Op::Mod, 1)),
            // 3.4: `*` is multiplication when the next non-space character is
            // alphanumeric, '(', '$' or '@'; otherwise it is the counter.
            (b'*', _) if self.star_is_multiply() => Some((Op::Mul, 1)),
            _ => None,
        }
    }

    fn star_is_multiply(&self) -> bool {
        let mut i = self.pos + 1;
        while matches!(self.s.get(i), Some(b' ') | Some(b'\t')) {
            i += 1;
        }
        match self.s.get(i) {
            Some(c) => c.is_ascii_alphanumeric() || *c == b'(' || *c == b'$' || *c == b'@',
            None => false,
        }
    }

    fn apply(&mut self, op: Op, a: i32, b: i32) -> i32 {
        let (ua, ub) = (a as u32, b as u32);
        match op {
            // 3.9: computed on the unsigned type and reinterpreted, so these
            // wrap quietly rather than overflowing.
            Op::Add => ua.wrapping_add(ub) as i32,
            Op::Sub => ua.wrapping_sub(ub) as i32,
            Op::Mul => ua.wrapping_mul(ub) as i32,
            // 3.9: signed, and guarded. Unguarded, `1/0` raised a fatal signal
            // in the original; INT_MIN / -1 traps on most hardware.
            Op::Div => {
                if b == 0 {
                    self.err(msg::DIVIDE_BY_ZERO, None);
                    0
                } else {
                    a.wrapping_div(b)
                }
            }
            Op::Mod => {
                if b == 0 {
                    self.err(msg::MODULO_BY_ZERO, None);
                    0
                } else {
                    a.wrapping_rem(b)
                }
            }
            Op::Shl => {
                if b < 0 {
                    self.err(msg::NEGATIVE_SHIFT, None);
                    0
                } else if b >= 32 {
                    0
                } else {
                    (ua << b) as i32
                }
            }
            Op::Shr => {
                if b < 0 {
                    self.err(msg::NEGATIVE_SHIFT, None);
                    0
                } else if b >= 32 {
                    a >> 31 // 0 for a non-negative value, -1 for a negative one
                } else {
                    a >> b
                }
            }
            Op::Lt => (a < b) as i32,
            Op::Gt => (a > b) as i32,
            Op::Le => (a <= b) as i32,
            Op::Ge => (a >= b) as i32,
            Op::Eq => (a == b) as i32,
            Op::Ne => (a != b) as i32,
            Op::And => (ua & ub) as i32,
            Op::Or => (ua | ub) as i32,
            Op::Xor => (ua ^ ub) as i32,
        }
    }

    // --- values -------------------------------------------------------------

    /// A value position: a chain of unary operators followed by a primary.
    /// 3.6: unary versus binary is decided positionally, and unary operators
    /// may be chained (`--5`, `~-1`).
    fn value(&mut self) -> Option<i32> {
        self.skip_ws();
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                let v = self.value()?;
                Some((v as u32).wrapping_neg() as i32)
            }
            Some(b'+') if !self.compat => {
                // 3.6: accepted in any value position in the default mode. In
                // compatibility mode a leading '+' works only via the zero
                // accumulator, so it is not accepted here.
                self.pos += 1;
                self.value()
            }
            Some(b'~') => {
                self.pos += 1;
                let v = self.value()?;
                Some(!(v as u32) as i32)
            }
            Some(b'!') => {
                self.pos += 1;
                let v = self.value()?;
                Some((v == 0) as i32)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Option<i32> {
        self.skip_ws();
        let c = self.peek()?;
        match c {
            b'(' => {
                self.pos += 1;
                let v = if self.compat {
                    self.accumulator_paren()
                } else {
                    self.expr(1)
                };
                self.skip_ws();
                if self.peek() == Some(b')') {
                    self.pos += 1;
                } else {
                    self.err(msg::PAREN_IMBALANCE, None);
                }
                Some(v)
            }
            // 3.2: `$` is a hex prefix only when a hex digit follows
            // immediately; otherwise it is the program counter (3.5).
            b'$' => {
                if self.at(1).is_some_and(|d| d.is_ascii_hexdigit()) {
                    self.pos += 1;
                    Some(self.radix(16))
                } else {
                    self.pos += 1;
                    Some(self.pc)
                }
            }
            b'%' => {
                self.pos += 1;
                // 3.2: `%` introduces a binary constant, and is NOT symmetric
                // with `$`, which becomes the counter when no hex digit
                // follows. A `%` with no binary digit is an empty binary
                // constant: diagnosed, yielding 0.
                //
                // --compatibility restores the original evaluation semantic --
                // the prefix contributes nothing and the value after it is
                // taken, so `%5` is 5 and `%(1<<2)` is 4. Real source relies on
                // that, writing `#%NAME` where NAME expands to `(1 << 2)`.
                match self.peek() {
                    Some(b'0') | Some(b'1') => Some(self.radix(2)),
                    _ if self.compat => match self.peek() {
                        Some(_) => self.primary(),
                        None => Some(self.radix(2)),
                    },
                    _ => {
                        self.err(msg::NO_BINARY_DIGIT, None);
                        Some(0)
                    }
                }
            }
            b'@' => {
                self.pos += 1;
                Some(self.radix(8))
            }
            // 3.4: the same lookahead that makes `*` multiplication in
            // operator position applies here. If it resolves to the operator,
            // it is not a value -- so an expression opening with `* 3` has no
            // left operand and yields nothing, rather than reading the counter
            // and multiplying it.
            b'*' if !self.star_is_multiply() => {
                self.pos += 1;
                Some(self.pc)
            }
            b'\'' => Some(self.char_const()),
            b'0'..=b'9' => Some(self.number()),
            _ if c.is_ascii_alphabetic() || c == b'_' || c == self.local_char => {
                Some(self.symbol())
            }
            _ => None,
        }
    }

    fn radix(&mut self, base: u32) -> i32 {
        let start = self.pos;
        let mut v: u32 = 0;
        while let Some(c) = self.peek() {
            match (c as char).to_digit(base) {
                Some(d) => {
                    v = v.wrapping_mul(base).wrapping_add(d);
                    self.pos += 1;
                }
                None => break,
            }
        }
        if self.pos == start {
            // No digits: `%` and `@` with nothing after them.
            self.err(msg::INVALID_TOKEN, None);
        }
        v as i32
    }

    /// 3.2: a suffixed constant must begin with a digit, so `FFh` is a symbol
    /// and `0FFh` is 255. There is no `0x` prefix and no leading-zero octal
    /// convention: `0x10` is the number 0 followed by the symbol `x10`.
    fn number(&mut self) -> i32 {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let run = &self.s[start..self.pos];
        let last = *run.last().unwrap_or(&b'0');
        let (base, body_len) = match last.to_ascii_lowercase() {
            b'h' => (16, run.len() - 1),
            b'b' => (2, run.len() - 1),
            b'q' | b'o' => (8, run.len() - 1),
            b'd' => (10, run.len() - 1),
            _ => (10, run.len()),
        };
        let body = &run[..body_len];
        let all_valid = !body.is_empty() && body.iter().all(|c| (*c as char).is_digit(base));
        if all_valid {
            let mut v: u32 = 0;
            for c in body {
                v = v
                    .wrapping_mul(base)
                    .wrapping_add((*c as char).to_digit(base).unwrap());
            }
            return v as i32;
        }
        // Not a well-formed suffixed constant: take only the leading decimal
        // digits and leave the rest to be scanned as something else.
        self.pos = start;
        let mut v: u32 = 0;
        while let Some(c) = self.peek() {
            match (c as char).to_digit(10) {
                Some(d) => {
                    v = v.wrapping_mul(10).wrapping_add(d);
                    self.pos += 1;
                }
                None => break,
            }
        }
        v as i32
    }

    /// 3.2: single quotes, exactly one character. Double quotes are not a
    /// value form -- they belong to .BYTE and .TEXT operand processing.
    fn char_const(&mut self) -> i32 {
        self.pos += 1; // opening quote
        let v = match self.peek() {
            Some(c) => {
                self.pos += 1;
                c as i32
            }
            None => {
                self.err(msg::PREMATURE_CHAR, None);
                return 0;
            }
        };
        if self.peek() == Some(b'\'') {
            self.pos += 1;
        }
        v
    }

    /// 3.7: starts with a letter, `_`, or the local-label character; continues
    /// with letters, digits, `_`, `.`, or the local-label character. A `.` is
    /// legal inside a symbol name.
    fn symbol(&mut self) -> i32 {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == self.local_char {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name = String::from_utf8_lossy(&self.s[start..self.pos]).into_owned();
        match (self.lookup)(&name) {
            Some(v) => v,
            None => {
                self.undefined.push(name);
                UNDEFINED
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> i32 {
        eval(text, 0, false, b'_', &mut |_| None).value
    }
    fn vc(text: &str) -> i32 {
        eval(text, 0, true, b'_', &mut |_| None).value
    }
    fn at(text: &str, pc: i32) -> i32 {
        eval(text, pc, false, b'_', &mut |_| None).value
    }

    /// The values tests/cases/expressions.asm asserts in its own comments.
    #[test]
    fn expressions_case_default_mode() {
        assert_eq!(v("1+2*3+4"), 11);
        assert_eq!(v("1+2*(3+4)"), 15);
        assert_eq!(v("2+3*4"), 14);
        assert_eq!(v("10-2-3"), 5); // left-associative
        assert_eq!(v("$1F"), 31);
        assert_eq!(v("%1010"), 10);
        assert_eq!(v("@17"), 15);
        assert_eq!(v("0FFh"), 255);
        assert_eq!(v("1010b"), 10);
        assert_eq!(v("100d"), 100);
        assert_eq!(v("0107q"), 71);
        assert_eq!(v("'A'"), 65);
        assert_eq!(v("5>3"), 1);
        assert_eq!(v("5<3"), 0);
        assert_eq!(v("5==5"), 1);
        assert_eq!(v("6&3"), 2);
        assert_eq!(v("4|1"), 5);
        assert_eq!(v("6^3"), 5);
        assert_eq!(v("1<<4"), 16);
        assert_eq!(v("32>>2"), 8);
        assert_eq!(v("17%5"), 2);
        assert_eq!(v("~0"), -1);
        assert_eq!(v("!0"), 1);
        assert_eq!(v("-1"), -1);
    }

    /// 3.1: the same three expressions under --compatibility. The switch's
    /// whole observable effect on arithmetic is these numbers.
    #[test]
    fn expressions_case_compatibility_mode() {
        assert_eq!(vc("1+2*3+4"), 13); // ((1+2)*3)+4
        assert_eq!(vc("1+2*(3+4)"), 21); // (1+2)*7
        assert_eq!(vc("2+3*4"), 20); // (2+3)*4
        assert_eq!(vc("10-2-3"), 5); // same in both modes
        assert_eq!(vc("-5"), -5); // works via the zero accumulator
    }

    #[test]
    fn shifts_bind_looser_than_addition() {
        // 3.1 places << >> below + -, which is unusual but matches C.
        assert_eq!(v("1<<2+3"), 32); // 1 << 5, not (1<<2)+3 == 7
    }

    #[test]
    fn program_counter_is_the_start_of_the_statement() {
        assert_eq!(at("$", 0x1234), 0x1234);
        assert_eq!(at("*", 0x1234), 0x1234);
        // 3.4: `*` followed by a non-alphanumeric is the counter, so `*+2`
        // is counter-plus-two while `* 3` is multiplication.
        assert_eq!(at("*+2", 0x100), 0x102);
        // `* 3` resolves to multiplication, not the counter -- so there is no
        // left operand and the expression yields 0 rather than 3*pc. The
        // corpus does not exercise this; 10.3's "Non-unary operator at
        // beginning of expression." is the diagnostic for it, gated on -a.
        assert_eq!(at("* 3", 4), 0);
        assert_eq!(at("2*3", 0), 6);
    }

    #[test]
    fn radix_forms_that_are_not_numbers() {
        // 3.2: no 0x prefix. `0x10` is the number 0 followed by symbol x10,
        // and with x10 undefined the expression stops at the 0.
        assert_eq!(v("0x10"), 0);
        // A suffixed constant must begin with a digit, so FFh is a symbol.
        let o = eval("FFh", 0, false, b'_', &mut |_| None);
        assert_eq!(o.undefined, vec!["FFh".to_string()]);
    }

    #[test]
    fn value_width_is_fixed_at_32_bits() {
        // negative-values.asm: these are pinned by the -l label table, the
        // only place a full-width value is printed.
        assert_eq!(v("-1") as u32, 0xFFFFFFFF);
        assert_eq!(v("-7") as u32, 0xFFFFFFF9);
        assert_eq!(v("-65536") as u32, 0xFFFF0000);
        assert_eq!(v("~0") as u32, 0xFFFFFFFF);
        assert_eq!(v("1<<31") as u32, 0x80000000);
        assert_eq!(v("32767"), 0x7FFF);
    }

    #[test]
    fn arithmetic_wraps_rather_than_overflowing() {
        // 3.9: computed on the unsigned type, so these are defined values.
        assert_eq!(v("2000000000+2000000000") as u32, 4_000_000_000u32);
        assert_eq!(v("100000*100000") as u32, 100_000u32.wrapping_mul(100_000));
        assert_eq!(v("-2147483648"), i32::MIN);
    }

    #[test]
    fn division_and_shift_hazards_are_guarded() {
        let o = eval("1/0", 0, false, b'_', &mut |_| None);
        assert_eq!(o.value, 0);
        assert_eq!(o.diags[0].msg, msg::DIVIDE_BY_ZERO);
        let o = eval("1%0", 0, false, b'_', &mut |_| None);
        assert_eq!(o.diags[0].msg, msg::MODULO_BY_ZERO);
        // INT_MIN / -1 is the other trap the original did not guard.
        assert_eq!(v("(1<<31)/-1"), i32::MIN);
        assert_eq!(v("(1<<31)%-1"), 0);
        // 3.9: a count of 32 or more yields 0, or -1 for >> of a negative.
        assert_eq!(v("1<<32"), 0);
        assert_eq!(v("1>>64"), 0);
        assert_eq!(v("-1>>64"), -1);
        let o = eval("1<<-1", 0, false, b'_', &mut |_| None);
        assert_eq!(o.diags[0].msg, msg::NEGATIVE_SHIFT);
    }

    #[test]
    fn unary_operators_chain_and_are_positional() {
        assert_eq!(v("--5"), 5);
        assert_eq!(v("~-1"), 0);
        assert_eq!(v("1 + +5"), 6); // unary + in any value position (default)
                                    // 3.6: in the default mode an operator with no right-hand value ends
                                    // the expression, so `5 ~ 3` is 5.
        assert_eq!(v("5 ~ 3"), 5);
        // In compatibility mode `~` in operator position replaces the
        // accumulator, so the same text is ~3.
        assert_eq!(vc("5 ~ 3"), !3i32);
    }

    #[test]
    fn undefined_symbols_take_the_sentinel() {
        let o = eval("nosuch", 0, false, b'_', &mut |_| None);
        assert_eq!(o.value, UNDEFINED);
        assert_eq!(o.undefined, vec!["nosuch".to_string()]);
        // 2.1: the sentinel is above the address space but its low 16 bits
        // are zero, which is why the shortening rules must test the full
        // value and not just the truncated one.
        // Compile-time, because both are properties of the constant rather
        // than of anything this test runs.
        const _: () = assert!(UNDEFINED & 0xFFFF == 0);
        const _: () = assert!(UNDEFINED >= 0x10000);
    }

    #[test]
    fn symbols_resolve_through_the_lookup() {
        let mut f = |n: &str| match n {
            "lab2" => Some(0x12),
            "lab3" => Some(0x1234),
            _ => None,
        };
        assert_eq!(eval("lab2+1", 0, false, b'_', &mut f).value, 0x13);
        assert_eq!(eval("lab3", 0, false, b'_', &mut f).value, 0x1234);
        // 3.7: a '.' is legal inside a symbol name.
        let mut g = |n: &str| if n == "a.b" { Some(7) } else { None };
        assert_eq!(eval("a.b", 0, false, b'_', &mut g).value, 7);
    }
}

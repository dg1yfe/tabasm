//! Encoding rules.
//!
//! 7.1: `argval` starts as the value of the first operand; the rule may
//! rewrite the opcode, `argval`, and both byte counts; then, unless the table
//! declared .NOARGSHIFT, the default step `argval = (argval << SHIFT) | OR`
//! runs; then bytes are emitted.

use crate::expr::Diag;
use crate::table::{self, Rule};

pub struct Enc {
    pub pcx: i32,
    pub opcode: u32,
    pub opcode_bytes: u8,
    pub arg_bytes: u8,
    pub argval: i32,
    pub shift: u32,
    pub or: u32,
    /// Evaluated operand values, in source order.
    pub argv: Vec<i32>,
    /// The operands' source text, which the shared range helpers report as
    /// the parenthesised detail.
    pub argt: Vec<String>,
    /// 7.21: the byte-vector output path, used only by I6's four-operand long
    /// index form, which needs five argument bytes and so cannot fit a value.
    pub vector: Option<Vec<u8>>,
    pub diags: Vec<Diag>,
}

impl Enc {
    /// `aval`: the low 16 bits of argval -- the first operand, truncated.
    fn aval(&self) -> u32 {
        self.argval as u32 & 0xFFFF
    }
    fn arg(&self, i: usize) -> i32 {
        self.argv.get(i).copied().unwrap_or(0)
    }
    /// A rule raising its own diagnostic clears the detail buffer first, so
    /// these carry no parenthesised text -- which is why the golden reads
    /// "Range of relative branch exceeded. " with a trailing space and nothing
    /// after it (7.1, and err-range.out).
    fn err(&mut self, msg: &'static str) {
        self.diags.push(Diag { msg, detail: None });
    }
    /// 7.17: the shared single-operand range check reports the offending
    /// operand's source text as detail.
    fn range(&mut self, i: usize) {
        let detail = self.argt.get(i).cloned();
        self.diags.push(Diag { msg: "Range of argument exceeded.", detail });
    }

    fn checked(&mut self, i: usize, lo: i32, hi: i32) -> i32 {
        let v = self.arg(i);
        if v < lo || v > hi {
            self.range(i);
        }
        v
    }

    /// The displacement rules all measure from the address FOLLOWING the whole
    /// instruction, which they compute by adding the two byte counts.
    fn delta(&self, target: i32) -> i32 {
        target - self.pcx - (self.opcode_bytes as i32 + self.arg_bytes as i32)
    }

    fn swap16(v: u32) -> i32 {
        (((v >> 8) & 0x00FF) | ((v << 8) & 0xFF00)) as i32
    }

    pub fn apply(&mut self, rule: Rule, noargshift: bool) {
        match rule {
            table::NO => {}
            table::JM => self.jm(),
            table::JT => self.jt(),
            table::R1 => self.r1(),
            table::ZP => self.zp(),
            table::MZ => self.mz(),
            table::MB => self.mb(),
            table::ZB => self.zb(),
            table::ZI => self.zi(),
            table::CO => self.co(),
            table::CR => self.cr(),
            table::CS => self.cs(),
            table::SW => self.sw(),
            table::R3REL => self.r3rel(),
            // Phase B and the unexercised 7.25 rules are not implemented yet.
            // 10.3: this is exactly what `Invalid MODOP.` is for.
            _ => self.err("Invalid MODOP."),
        }
        // 5.5: the default shift/OR step, unless the table opted out.
        if !noargshift {
            self.argval = ((self.argval as u32) << (self.shift & 31) | self.or) as i32;
        }
    }

    // --- 7.4 JM: jump within a 2K page -------------------------------------
    fn jm(&mut self) {
        let aval = self.aval();
        // The OR column is NOT OR-ed in here: it is a mask selecting which
        // high bits must agree, so tasm48.tab's 0000 disables the check
        // entirely (the 8048 takes high address bits from SEL MB instead).
        if ((self.pcx as u32).wrapping_add(2) & self.or) != (aval & self.or) {
            self.err("Branch off of current 2K page.");
        }
        self.opcode |= (aval & 0x700) >> 3;
        self.argval = (aval & 0xFF) as i32;
    }

    // --- 7.5 JT: jump within the current 256-byte page ---------------------
    fn jt(&mut self) {
        let aval = self.aval();
        // pcx + 1, not pcx: an instruction starting at XXFF has its operand
        // byte on the following page, and the target must be on that page.
        if (aval & 0xFF00) != ((self.pcx as u32).wrapping_add(1) & 0xFF00) {
            self.err("Branch off of current page.");
        }
        self.argval = (aval & 0xFF) as i32;
    }

    // --- 7.6 R1: 8-bit PC-relative -----------------------------------------
    fn r1(&mut self) {
        let d = self.delta(self.aval() as i32);
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err("Range of relative branch exceeded.");
        } else {
            self.argval = d & 0xFF;
        }
    }

    // --- 7.7 ZP: 6502 zero page --------------------------------------------
    fn zp(&mut self) {
        // The first test is against the FULL value, which is what catches the
        // 0x20000 undefined sentinel: its low 16 bits are zero, so the second
        // test alone would wrongly shorten every forward reference (2.1).
        if (self.argval as i64) < 0x10000 && self.aval() < 0x100 {
            self.opcode = match self.opcode {
                0x9C => 0x64, // STZ abs
                0x9E => 0x74, // STZ abs,X
                _ => self.opcode & 0xF7, // clear the absolute-addressing bit
            };
            self.arg_bytes = 1;
        }
    }

    // --- 7.8 MZ: Motorola zero page ----------------------------------------
    fn mz(&mut self) {
        if (self.argval as i64) < 0x10000 && self.aval() <= 0xFF {
            self.opcode = match self.opcode & 0xF0 {
                0xC0 => (self.opcode & 0x0F) | 0xB0, // 6805 extended -> direct
                0xD0 => (self.opcode & 0x0F) | 0xE0, // 6805 indexed 2 -> 1 byte
                _ => self.opcode & 0xFFDF,           // 6800 family zero page
            };
            self.arg_bytes = 1;
        } else {
            // These are big-endian parts and the emitter writes low byte
            // first, so the two bytes are swapped here. ZP has no such branch
            // because the 6502 is little-endian.
            self.argval = Self::swap16(self.aval());
        }
    }

    // --- 7.9 MB: 6805 bit instructions -------------------------------------
    fn mb(&mut self) {
        let bit = self.arg(0);
        self.opcode |= ((bit as u32) & 0x7) << 1;
        if self.argv.len() >= 3 {
            let d = self.arg(2) - self.pcx - 3;
            if !(-128..=127).contains(&d) {
                self.argval = 0;
                self.err("Range of relative branch exceeded.");
            } else {
                // The displacement occupies the HIGH byte, so the zero-page
                // address is written first by the low-byte-first emitter.
                self.argval = (((d & 0xFF) << 8) | (self.arg(1) & 0xFF)) as i32;
            }
        } else {
            self.argval = self.arg(1) & 0xFF;
        }
    }

    // --- 7.10 ZB: Z80 bit instructions -------------------------------------
    fn zb(&mut self) {
        let bit = self.checked(0, 0, 7);
        if self.argv.len() == 1 {
            // Bits 11-13 place the bit number in the second opcode byte of
            // the CB-prefixed encodings.
            self.opcode |= ((bit as u32) & 0x7) << 11;
            self.argval = 0;
        } else {
            let disp = self.checked(1, -128, 127);
            self.argval = ((((bit as u32) & 0x7) << 11) | ((disp as u32) & 0xFF)) as i32;
        }
    }

    // --- 7.11 ZI: Z80 indexed ----------------------------------------------
    fn zi(&mut self) {
        let disp = self.checked(0, -128, 127);
        if self.argv.len() == 1 {
            // Truncated to one byte deliberately: sign-extension would
            // collide with the high byte some four-byte encodings OR in.
            self.argval = disp & 0xFF;
        } else {
            // Note the asymmetric range: a signed OR an unsigned byte.
            let data = self.checked(1, -128, 255);
            self.argval = (((disp as u32) & 0xFF) | (((data as u32) & 0xFF) << 8)) as i32;
        }
    }

    // --- 7.12 CO: combine two operands -------------------------------------
    fn co(&mut self) {
        // Keyed on the ARGUMENT BYTE COUNT, not the operand count.
        if self.arg_bytes == 2 {
            self.argval = ((self.aval() & 0xFF) | (((self.arg(1) as u32) & 0xFF) << 8)) as i32;
        } else {
            self.argval =
                ((((self.arg(1) as u32) & 0xFF) << 16) | (self.argval as u32 & 0xFFFF)) as i32;
        }
    }

    // --- 7.13 CS: combine two operands, swapped -----------------------------
    fn cs(&mut self) {
        let a = self.aval();
        if self.arg_bytes == 2 {
            self.argval = (((self.arg(1) as u32) & 0xFF) | ((a & 0xFF) << 8)) as i32;
        } else {
            self.argval = ((((self.arg(1) as u32) & 0xFF) << 16)
                | ((a & 0x00FF) << 8)
                | ((a & 0xFF00) >> 8)) as i32;
        }
        // Here the OR column is a VALIDATION mask, not a value to OR in.
        if self.or != 0 && (self.argval as u32) != (self.argval as u32 & self.or) {
            let detail = Some(format!(
                "{} or {}",
                self.argt.first().cloned().unwrap_or_default(),
                self.argt.get(1).cloned().unwrap_or_default()
            ));
            self.diags.push(Diag { msg: "range of argument exceeded.", detail });
        }
    }

    // --- 7.14 CR: combine, second operand relative --------------------------
    fn cr(&mut self) {
        let d = self.delta(self.arg(1));
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err("Range of relative branch exceeded.");
        } else {
            self.argval = ((self.argval as u32 & 0xFF) | (((d as u32) & 0xFF) << 8)) as i32;
        }
    }

    // --- 7.15 SW: byte swap -------------------------------------------------
    fn sw(&mut self) {
        // Turns a 16-bit value big-endian so the low-byte-first emitter writes
        // it high byte first. How 8051 LCALL, LJMP and MOV DPTR,#nn work.
        self.argval = Self::swap16(self.aval());
    }

    // --- 7.16 3R: three operands, third relative ----------------------------
    fn r3rel(&mut self) {
        let d = self.delta(self.arg(2));
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            // Lower-case 'r' here, upper-case in R1/MB/CR. Inconsistent in the
            // original; the reference output preserves it.
            self.err("range of relative branch exceeded.");
        } else {
            self.argval = ((self.aval() & 0xFF)
                | (((self.arg(1) as u32) & 0xFF) << 8)
                | (((d as u32) & 0xFF) << 16)) as i32;
        }
    }
}

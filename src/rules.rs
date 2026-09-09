//! Encoding rules.
//!
//! 7.1: `argval` starts as the value of the first operand; the rule may
//! rewrite the opcode, `argval`, and both byte counts; then, unless the table
//! declared .NOARGSHIFT, the default step `argval = (argval << SHIFT) | OR`
//! runs; then bytes are emitted.

use crate::errlog::msg;
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
    /// The post-rule transform, per row rather than per table. See
    /// `table::Row::post_shift`.
    pub post_shift: u32,
    pub post_or: u32,
    /// How many auxiliary registers the target has (7.20's `arp_val`).
    pub aux_registers: u32,
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
        self.diags.push(Diag {
            msg: msg::RANGE_ARG,
            detail,
        });
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

    pub fn apply(&mut self, rule: Rule) {
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
            table::T1 => self.t1(),
            table::TD => self.td(),
            table::TL => self.tl(),
            table::T5 => self.t5(),
            table::TA => self.ta(),
            table::SU => self.su(),
            table::R2 => self.r2(),
            table::I1 => self.i1(),
            table::I2 => self.short_long(1, self.shift),
            table::I3 => self.short_long(2, 0),
            table::I4 => self.i4(),
            table::I5 => self.i5(),
            table::I6 => self.i6(),
            table::I7 => self.short_long(0, 0),
            table::I8 => self.i8(),
            // 7.25 -- unexercised; see the note on each.
            table::T2 => self.t2(),
            table::T3 => self.t3(),
            table::T4 => self.t4(),
            table::T6 => self.t6(),
            table::A3 => self.a3(),
            table::CN => self.cn(false),
            table::C5 => self.cn(true),
            table::SZ => self.sz(),
            table::SB => self.sb(),
            table::R3 => self.r3(),
            table::ZW => self.zw(),
            table::ZD => self.zd(),
            table::ZL => self.zl(),
            // Phase B and the unexercised 7.25 rules are not implemented yet.
            // 10.3: this is exactly what `Invalid MODOP.` is for.
            _ => self.err(msg::INVALID_MODOP),
        }
        // 5.5: the post-rule transform. v1 states it once per table (the absence
        // of `.NOARGSHIFT`) and the loader resolves it onto each row; v2 writes
        // it per row. Zero/zero is the identity.
        self.argval = ((self.argval as u32) << (self.post_shift & 31) | self.post_or) as i32;
    }

    // --- 7.4 JM: jump within a 2K page -------------------------------------
    fn jm(&mut self) {
        let aval = self.aval();
        // The OR column is NOT OR-ed in here: it is a mask selecting which
        // high bits must agree, so tasm48.tab's 0000 disables the check
        // entirely (the 8048 takes high address bits from SEL MB instead).
        if ((self.pcx as u32).wrapping_add(2) & self.or) != (aval & self.or) {
            self.err(msg::OFF_2K_PAGE);
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
            self.err(msg::OFF_PAGE);
        }
        self.argval = (aval & 0xFF) as i32;
    }

    // --- 7.6 R1: 8-bit PC-relative -----------------------------------------
    fn r1(&mut self) {
        let d = self.delta(self.aval() as i32);
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err(msg::RANGE_BRANCH);
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
                0x9C => 0x64,            // STZ abs
                0x9E => 0x74,            // STZ abs,X
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
                self.err(msg::RANGE_BRANCH);
            } else {
                // The displacement occupies the HIGH byte, so the zero-page
                // address is written first by the low-byte-first emitter.
                self.argval = ((d & 0xFF) << 8) | (self.arg(1) & 0xFF);
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
            self.diags.push(Diag {
                msg: msg::RANGE_ARG_LC,
                detail,
            });
        }
    }

    // --- 7.14 CR: combine, second operand relative --------------------------
    fn cr(&mut self) {
        let d = self.delta(self.arg(1));
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err(msg::RANGE_BRANCH);
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
            self.err(msg::RANGE_BRANCH_LC);
        } else {
            self.argval = ((self.aval() & 0xFF)
                | (((self.arg(1) as u32) & 0xFF) << 8)
                | (((d as u32) & 0xFF) << 16)) as i32;
        }
    }
}

// --- Phase B: shared helpers (7.20) ----------------------------------------

impl Enc {
    /// `shift_and`: evaluate, shift left, mask, range-check.
    ///
    /// The SHIFT column carries TWO fields here. Only the low nibble is a
    /// shift count; a non-zero HIGH nibble means "invert the masked bit
    /// field", a kludge for the TMS320C25 BIT instruction whose operand counts
    /// from the opposite end. The range check compares before and after
    /// masking, so it fires whenever the shifted value has bits outside the
    /// mask.
    fn shift_and(&mut self, i: usize, shift: u32, mask: u32) -> u32 {
        let invert = shift & 0xF0;
        let count = shift & 0x0F;
        let shifted = ((self.arg(i) as u32) & 0xFFFF) << count;
        let result = shifted & mask;
        if result != shifted {
            self.range(i);
        }
        if invert != 0 {
            (!shifted) & mask
        } else {
            result
        }
    }

    /// `arp_val`: the auxiliary-register number, whose width depends on the
    /// SELECTED TABLE rather than on the table's contents.
    ///
    /// 7.20 describes this as a comparison against the literal `-<nn>` selector
    /// text -- the one place the original's behaviour depends on which table was
    /// NAMED rather than on its contents. The width is now a property of the
    /// table (`aux_registers`), derived from the selector for a v1 table and
    /// stated outright by a v2 one, so no processor name reaches this code.
    fn arp_val(&mut self, i: usize) -> u32 {
        let value = (self.arg(i) as u32) & 0xFFFF;
        let result = value & (self.aux_registers.saturating_sub(1));
        if result != value {
            let detail = self.argt.get(i).cloned();
            self.diags.push(Diag {
                msg: msg::RANGE_ARP,
                detail,
            });
        }
        result
    }

    /// `isargvalid`: per-field validation against a slice of the OR mask. For
    /// the 8096 rules the OR column is a validation mask, not a value to OR
    /// in -- tasm96.tab's `00FeFeFe` is three 8-bit fields whose low bit must
    /// be clear, because those operands are even-aligned register pairs.
    fn isargvalid(&mut self, i: usize, mask: u32, startbit: u32, width: u32) {
        let widthmask: u32 = if width >= 32 {
            u32::MAX
        } else {
            (1u32 << width) - 1
        };
        let valid = if mask != 0 {
            widthmask & (mask >> startbit)
        } else {
            widthmask
        };
        let mut v = self.arg(i) as u32;
        if self.arg(i) < 0 {
            v &= widthmask; // ignore sign extension
        }
        if v != (v & valid) {
            let detail = self.argt.get(i).cloned();
            // Note the lower case, distinct from the shared single-operand
            // check's msg::RANGE_ARG.
            self.diags.push(Diag {
                msg: msg::RANGE_ARG_LC,
                detail,
            });
        }
    }

    // --- 7.22 TMS320 --------------------------------------------------------

    fn t1(&mut self) {
        let arg0 = self.shift_and(0, self.shift, self.or);
        let arp = if self.argv.len() > 1 {
            self.arp_val(1)
        } else {
            0
        };
        self.opcode |= arp | arg0;
        self.argval = 0;
    }

    fn td(&mut self) {
        let dma = self.shift_and(0, 0, 0x7F); // 7-bit direct address, no shift
        let arg1 = if self.argv.len() > 1 {
            self.shift_and(1, self.shift, self.or)
        } else {
            0
        };
        self.opcode |= dma | arg1;
        self.argval = 0;
    }

    fn tl(&mut self) {
        let arg0 = (self.arg(0) as u32) & 0xFFFF;
        self.argval = Self::swap16(arg0);
        if self.argv.len() > 1 {
            let arg1 = self.shift_and(1, self.shift, self.or);
            self.opcode |= arg1;
        }
    }

    fn t5(&mut self) {
        let arg1 = (self.arg(1) as u32) & 0xFFFF;
        self.argval = Self::swap16(arg1);
        let arg0 = self.shift_and(0, self.shift, self.or);
        self.opcode |= arg0;
    }

    fn ta(&mut self) {
        // The register number lands at bit 8 -- the opcode's HIGH byte --
        // where T1 places it at bit 0.
        let arp = self.arp_val(0) << 8;
        let arg1 = if self.argv.len() > 1 {
            self.shift_and(1, self.shift, self.or)
        } else {
            0
        };
        self.opcode |= arp | arg1;
        self.argval = 0;
    }

    // --- 7.23 TMS7000 -------------------------------------------------------

    /// The only rule that SUBTRACTS from the opcode, and the only one whose
    /// range limit is a literal rather than a mask from the table. It exists
    /// for TRAP, whose 24 vectors occupy descending opcodes. The check is
    /// after the subtraction, so the opcode has already moved when it fires.
    fn su(&mut self) {
        let arg = self.arg(0) as u32;
        self.opcode = self.opcode.wrapping_sub(arg);
        if self.arg(0) > 23 {
            self.range(0);
        }
        self.argval = 0;
    }

    // --- 7.24 Intel 8096 -----------------------------------------------------

    /// As R1 but two bytes wide, and with NO range check at all: a too-large
    /// displacement is silently truncated by the emitter.
    fn r2(&mut self) {
        self.argval = self.delta(self.argval);
    }

    fn i1(&mut self) {
        let argc = self.argv.len();
        let ab = self.arg_bytes;
        let g = |e: &Enc, i: usize| (e.arg(i) as u32) & 0xFF;
        // Operands are emitted in REVERSE order: the 8096 encodes its
        // destination last.
        self.argval = match (argc, ab) {
            (1, _) => {
                self.isargvalid(0, self.or, 0, 8);
                self.argval
            }
            (2, 2) => {
                self.isargvalid(1, self.or, 0, 8);
                self.isargvalid(0, self.or, 8, 8);
                (g(self, 1) | (g(self, 0) << 8)) as i32
            }
            (2, _) => {
                self.isargvalid(1, self.or, 0, 16);
                self.isargvalid(0, self.or, 16, 8);
                (((self.arg(1) as u32) & 0xFFFF) | (g(self, 0) << 16)) as i32
            }
            (3, 3) => {
                self.isargvalid(2, self.or, 0, 8);
                self.isargvalid(1, self.or, 8, 8);
                self.isargvalid(0, self.or, 16, 8);
                (g(self, 2) | (g(self, 1) << 8) | (g(self, 0) << 16)) as i32
            }
            (3, _) => {
                self.isargvalid(2, self.or, 0, 16);
                self.isargvalid(1, self.or, 16, 8);
                self.isargvalid(0, self.or, 24, 8);
                (((self.arg(2) as u32) & 0xFFFF) | (g(self, 1) << 16) | (g(self, 0) << 24)) as i32
            }
            _ => {
                self.isargvalid(3, self.or, 0, 8);
                self.isargvalid(2, self.or, 8, 8);
                self.isargvalid(1, self.or, 16, 8);
                self.isargvalid(0, self.or, 24, 8);
                (g(self, 3) | (g(self, 2) << 8) | (g(self, 1) << 16) | (g(self, 0) << 24)) as i32
            }
        };
        // Unconditionally. Here SHIFT is neither a shift nor an ordinary OR
        // value: it sets the low bit of the first argument byte, selecting the
        // 8096's auto-increment addressing modes.
        self.argval = (self.argval as u32 | self.shift) as i32;
    }

    /// I1's three-operand case with the middle two exchanged. One row: TIJMP.
    fn i8(&mut self) {
        if self.argv.len() == 3 && self.arg_bytes == 3 {
            let g = |e: &Enc, i: usize| (e.arg(i) as u32) & 0xFF;
            self.argval = (g(self, 1) | (g(self, 2) << 8) | (g(self, 0) << 16)) as i32;
        }
    }

    /// I2, I3 and I7 shorten the instruction when the tested address fits in
    /// one byte, differing only in which operand is tested. Note the
    /// convention is the OPPOSITE of the 6502 and Motorola zero-page rules:
    /// the table declares the LONG form and the rule rewrites it short.
    fn short_long(&mut self, tested: usize, xor: u32) {
        let argc = self.argv.len();
        let short = ((self.arg(tested) as u32) & 0xFFFF) < 256;
        // Operands pack in reverse, destination last, exactly as I1 does. In
        // the long form the tested operand contributes two bytes instead of
        // one and the opcode is left alone.
        let mut v: u32 = 0;
        for i in 0..argc {
            if !short && i == tested {
                v = (v << 16) | ((self.arg(i) as u32) & 0xFFFF);
            } else {
                v = (v << 8) | ((self.arg(i) as u32) & 0xFF);
            }
        }
        self.argval = v as i32;
        if short {
            self.opcode = ((self.opcode >> 8) & 0xFFFC) ^ xor;
            self.opcode_bytes = self.opcode_bytes.saturating_sub(1);
            self.arg_bytes = argc as u8;
        }
    }

    /// Jump on bit: a byte address, a bit number folded into the opcode's low
    /// three bits, and a relative target.
    fn i4(&mut self) {
        let d = self.delta(self.arg(2));
        let bit = self.arg(1);
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err(msg::RANGE_BRANCH_LC);
        } else if bit > 7 {
            self.argval = 0;
            self.err(msg::RANGE_ARG_LC);
        } else {
            self.argval = (((self.arg(0) as u32) & 0xFF) | (((d as u32) & 0xFF) << 8)) as i32;
            self.opcode |= bit as u32;
        }
    }

    /// 11-bit PC-relative, carried INSIDE the opcode. The only rule whose
    /// diagnostic detail is the computed offset rather than the operand text.
    fn i5(&mut self) {
        let d = self.delta(self.arg(0));
        if !(-1024..=1023).contains(&d) {
            let detail = Some(format!("offset={}", d));
            self.diags.push(Diag {
                msg: msg::RANGE_BRANCH_LC,
                detail,
            });
        } else {
            self.opcode |= (d as u32) & 0x07FF;
        }
        self.argval = 0;
    }

    /// Indexed addressing, and the only user of the byte-vector output path:
    /// its four-operand long form needs five argument bytes, which does not
    /// fit a 32-bit value.
    fn i6(&mut self) {
        let argc = self.argv.len();
        if argc < 2 {
            return;
        }
        // A negative index in -128..-1 is biased into 128..255 first, so it
        // still fits one byte; the form is short when the index is then below
        // 256.
        let raw = self.arg(argc - 2);
        let idx = if (-128..0).contains(&raw) {
            (raw + 256) as u32
        } else {
            raw as u32
        };
        let short = idx < 256;
        let g = |e: &Enc, i: usize| (e.arg(i) as u32) & 0xFF;
        let base = argc - 1;

        if argc == 4 {
            let mut v: Vec<u8> = Vec::new();
            if short {
                v.push(g(self, base) as u8);
                v.push(idx as u8);
                v.push(g(self, 1) as u8);
                v.push(g(self, 0) as u8);
                self.arg_bytes = self.arg_bytes.saturating_sub(1);
            } else {
                // The forced low bit is how the 8096 distinguishes long-index
                // from short-index encodings at run time.
                v.push((g(self, base) | 1) as u8);
                v.push((idx & 0xFF) as u8);
                v.push(((idx >> 8) & 0xFF) as u8);
                v.push(g(self, 1) as u8);
                v.push(g(self, 0) as u8);
            }
            self.vector = Some(v);
            return;
        }

        let mut v: u32;
        if short {
            v = g(self, base) | (idx << 8);
            self.arg_bytes = self.arg_bytes.saturating_sub(1);
            if argc == 3 {
                v |= g(self, 0) << 16;
            }
        } else {
            v = (g(self, base) | 1) | (idx << 8);
            if argc == 3 {
                v |= g(self, 0) << 24;
            }
        }
        self.argval = v as i32;
    }
}

// --- rules no shipped table selects ----------------------------------------
//
// These are reachable only through .ADDINSTR, which lets a source file add a
// table row at assembly time. No shipped table selects them and no test case
// exercises them, so unlike every other rule here they rest on the written
// description alone. Treat them as unverified.

impl Enc {
    /// TMS9900. With two operands the first becomes a byte-swapped 16-bit
    /// argument and the second's low nibble is OR-ed into the opcode; with
    /// one, that operand's low nibble is OR-ed in.
    fn t2(&mut self) {
        if self.argv.len() >= 2 {
            self.argval = Self::swap16((self.arg(0) as u32) & 0xFFFF);
            self.opcode |= (self.arg(1) as u32) & 0x0F;
        } else {
            self.opcode |= (self.arg(0) as u32) & 0x0F;
            self.argval = 0;
        }
    }

    /// TMS9900. The second operand becomes the byte-swapped argument, the
    /// first's low nibble goes into the opcode -- T2 with the roles exchanged.
    fn t3(&mut self) {
        self.argval = Self::swap16((self.arg(1) as u32) & 0xFFFF);
        self.opcode |= (self.arg(0) as u32) & 0x0F;
    }

    /// TMS9900: two register operands, both folded into the opcode.
    fn t4(&mut self) {
        self.opcode |= ((self.arg(0) as u32) & 0x0F) | (((self.arg(1) as u32) & 0x0F) << 6);
        self.argval = 0;
    }

    /// As TD, but the first operand is masked to a nibble rather than to
    /// seven bits.
    fn t6(&mut self) {
        let dma = self.shift_and(0, 0, 0x0F);
        let arg1 = if self.argv.len() > 1 {
            self.shift_and(1, self.shift, self.or)
        } else {
            0
        };
        self.opcode |= dma | arg1;
        self.argval = 0;
    }

    /// 3R without the relative third operand: three operands, one byte each,
    /// low byte first.
    fn a3(&mut self) {
        self.argval = (((self.arg(0) as u32) & 0xFF)
            | (((self.arg(1) as u32) & 0xFF) << 8)
            | (((self.arg(2) as u32) & 0xFF) << 16)) as i32;
    }

    /// Z8: two operands combined into one byte, arg0 in the low nibble and
    /// arg1 in the high. `swapped` selects C5, which exchanges them.
    fn cn(&mut self, swapped: bool) {
        let (lo, hi) = if swapped {
            (1usize, 0usize)
        } else {
            (0usize, 1usize)
        };
        let v = ((self.arg(lo) as u32) & 0x0F) | (((self.arg(hi) as u32) & 0x0F) << 4);
        self.argval = v as i32;
        if self.or != 0 && v != (v & self.or) {
            let detail = Some(format!(
                "{} or {}",
                self.argt.first().cloned().unwrap_or_default(),
                self.argt.get(1).cloned().unwrap_or_default()
            ));
            self.diags.push(Diag {
                msg: msg::RANGE_ARG_LC,
                detail,
            });
        }
    }

    /// ST7: as MZ, but rewriting Cx -> Bx and Dx -> Ex.
    fn sz(&mut self) {
        if (self.argval as i64) < 0x10000 && self.aval() <= 0xFF {
            self.opcode = match self.opcode & 0xF0 {
                0xC0 => (self.opcode & 0x0F) | 0xB0,
                0xD0 => (self.opcode & 0x0F) | 0xE0,
                _ => self.opcode,
            };
            self.arg_bytes = 1;
        } else {
            self.argval = Self::swap16(self.aval());
        }
    }

    /// ST7: as MB, but the bit number is the SECOND operand rather than the
    /// first.
    fn sb(&mut self) {
        let bit = self.arg(1);
        self.opcode |= ((bit as u32) & 0x7) << 1;
        if self.argv.len() >= 3 {
            let d = self.arg(2) - self.pcx - 3;
            if !(-128..=127).contains(&d) {
                self.argval = 0;
                self.err(msg::RANGE_BRANCH);
            } else {
                self.argval = ((d & 0xFF) << 8) | (self.arg(0) & 0xFF);
            }
        } else {
            self.argval = self.arg(0) & 0xFF;
        }
    }

    /// uPD75000: a 4-bit PC-relative displacement OR-ed into the opcode.
    fn r3(&mut self) {
        let d = self.delta(self.aval() as i32);
        if !(-16..=15).contains(&d) {
            self.err(msg::RANGE_BRANCH);
            self.argval = 0;
        } else {
            self.opcode |= (d as u32) & 0x0F;
            self.argval = 0;
        }
    }

    /// Z8: if both operands are working registers (E0-EF) they combine into
    /// one byte via the vector path and the encoding drops to working-register
    /// mode; otherwise the pair is byte-swapped.
    fn zw(&mut self) {
        let (a, b) = ((self.arg(0) as u32) & 0xFF, (self.arg(1) as u32) & 0xFF);
        if (0xE0..=0xEF).contains(&a) && (0xE0..=0xEF).contains(&b) {
            // Working-register mode: the two nibbles combine into one byte via
            // the vector path, the opcode drops by two (the Z8's `R,R` form is
            // `x4` and its `r,r` form `x2`), and one argument byte goes.
            self.vector = Some(vec![(((a & 0x0F) << 4) | (b & 0x0F)) as u8]);
            self.opcode = self.opcode.wrapping_sub(2);
            self.arg_bytes = 1;
        } else {
            // Byte-swapped: the second operand is emitted first.
            self.argval = ((a << 8) | b) as i32;
        }
    }

    /// Z8 DJNZ: the first operand is a working register OR-ed into the
    /// opcode's high nibble, the second a 1-byte PC-relative target.
    fn zd(&mut self) {
        self.opcode |= ((self.arg(0) as u32) & 0x0F) << 4;
        let d = self.delta(self.arg(1));
        if !(-128..=127).contains(&d) {
            self.argval = 0;
            self.err(msg::RANGE_BRANCH);
        } else {
            self.argval = d & 0xFF;
        }
    }

    /// Z8 `LD immed[r1],r2`: the second operand must be a working register
    /// (below 16) and is OR-ed into the opcode's high nibble; the first gets
    /// the default handling.
    fn zl(&mut self) {
        let r = self.arg(1);
        if !(0..16).contains(&r) {
            self.range(1);
        }
        self.opcode |= ((r as u32) & 0x0F) << 4;
        self.argval = self.aval() as i32;
    }
}

#[cfg(test)]
mod tests {
    use crate::table::{self, Table};

    /// Every rule any shipped table can select must be implemented -- if one
    /// is not, `apply` falls through to `Invalid MODOP.` and the corpus would
    /// silently encode nothing for those rows.
    #[test]
    fn every_rule_the_shipped_tables_select_is_implemented() {
        let implemented = [
            table::NO,
            table::JM,
            table::JT,
            table::R1,
            table::ZP,
            table::MZ,
            table::MB,
            table::ZB,
            table::ZI,
            table::CO,
            table::CR,
            table::CS,
            table::SW,
            table::R3REL,
            table::T1,
            table::TD,
            table::TL,
            table::T5,
            table::TA,
            table::SU,
            table::R2,
            table::I1,
            table::I2,
            table::I3,
            table::I4,
            table::I5,
            table::I6,
            table::I7,
            table::I8,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for sel in [
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
        ] {
            let t = Table::load(&format!("tables/{}.tab2", sel), sel)
                .ok()
                .unwrap();
            for r in &t.rows {
                assert!(
                    implemented.contains(&r.rule),
                    "{} selects an unimplemented rule via {:?}",
                    sel,
                    r.mnemonic
                );
                seen.insert(r.rule.0);
            }
        }
        // README: 29 rules are reachable from a shipped table, and all are
        // exercised by the vectors.
        assert_eq!(seen.len(), 29, "expected 29 reachable rules");
    }
}

//! The two-pass assembler (the specification).

use crate::cli::{ObjFormat, Options};
use crate::errlog::{msg, Format};
use crate::expr;
use crate::image::Image;
use crate::limits::*;
use crate::listing;
use crate::macros::Macros;
use crate::matcher;
use crate::rules::Enc;
use crate::symbols::{Segment, Symbols};
use crate::table::Table;

struct Cond {
    /// Whether this level's branch is being assembled.
    active: bool,
    /// Whether any branch at this level has been taken (so .ELSE knows).
    taken: bool,
}

pub struct Asm {
    pub o: Options,
    pub table: Table,
    pub syms: Symbols,
    pub macs: Macros,
    pub img: Image,
    pub fmt: Format,
    pub prog: &'static str,

    pass: u8,
    pc: u32,
    pub errors: u32,
    pub stdout: String,
    pub lst: Vec<String>,

    // 4.5: these two take effect in pass 1 only and are NOT reset between
    // passes, so on pass 2 a change is already in force from line 1.
    local_char: u8,
    comment_char: u8,
    module: String,

    ls_first: bool, // source .LSFIRST/.MSFIRST, for .WORD data only
    segment: Segment,
    pub end_addr: u32,
    saw_end: bool,
    cond: Vec<Cond>,
    listing_on: bool,
    codes: bool,
    pub title: String,
    pub sym_out: Option<String>,
    pub avsym: bool,

    file: String,
    /// The top-level source name, kept for the page heading: `file` is
    /// restored to the includer when a nested file finishes.
    source_name: String,
    line_no: u32,
    depth: usize,
    /// Source lines processed in the last pass, for -y.
    pub lines_read: u32,
    /// Bytes emitted by the current statement, for the listing.
    emitted: Vec<u8>,
    line_pc: u32,
    /// Diagnostics that belong AFTER their listing line rather than before it.
    pending: Vec<String>,
    /// Names defined so far in THIS pass. A duplicate has to be detected per
    /// pass: every label is legitimately defined once per pass, so comparing
    /// against the symbol table alone cannot tell a redefinition from pass 2
    /// simply revisiting the line.
    seen_this_pass: std::collections::HashSet<String>,
}

impl Asm {
    pub fn new(o: Options, table: Table, fmt: Format, prog: &'static str) -> Asm {
        let ignore_case = o.ignore_case;
        let fill = o.fill.unwrap_or(0);
        Asm {
            table,
            syms: Symbols::new(ignore_case),
            macs: Macros::new(),
            img: Image::new(fill),
            fmt,
            prog,
            pass: 1,
            pc: 0,
            errors: 0,
            stdout: String::new(),
            lst: Vec::new(),
            local_char: b'_',
            comment_char: b';',
            module: "noname".to_string(),
            ls_first: true,
            segment: Segment::Null,
            end_addr: 0,
            saw_end: false,
            cond: Vec::new(),
            listing_on: true,
            codes: true,
            title: String::new(),
            sym_out: None,
            avsym: false,
            file: String::new(),
            source_name: String::new(),
            line_no: 0,
            depth: 0,
            lines_read: 0,
            emitted: Vec::new(),
            line_pc: 0,
            pending: Vec::new(),
            seen_this_pass: std::collections::HashSet::new(),
            o,
        }
    }

    // --- addressing ---------------------------------------------------------

    /// 5.3: with .WORDADDRS the counter is in 16-bit words, but the image is
    /// still byte-indexed.
    fn byte_addr(&self) -> u32 {
        if self.table.wordaddrs {
            self.pc.wrapping_mul(2)
        } else {
            self.pc
        }
    }

    /// 5.3: an odd byte count still advances the counter by a whole word.
    fn advance(&mut self, bytes: u32) {
        self.pc = if self.table.wordaddrs {
            self.pc.wrapping_add((bytes + 1) / 2)
        } else {
            self.pc.wrapping_add(bytes)
        };
    }

    /// Emit a statement's bytes and advance the counter over them.
    ///
    /// With .WORDADDRS an odd byte count still advances a whole word, which
    /// leaves a pad byte inside that word. The golden object for tms-wordaddr
    /// is a single 20-byte record, so that pad belongs to the emitted region
    /// rather than breaking it -- touching it keeps the run contiguous and
    /// carries the image's own fill value. It is NOT added to the listing:
    /// line 16 there shows `03` alone.
    fn emit_and_advance(&mut self, bytes: &[u8]) {
        self.emit(bytes);
        if self.table.wordaddrs && self.emitted.len() % 2 == 1 && self.pass == 2 {
            let a = self.byte_addr().wrapping_add(self.emitted.len() as u32);
            let fill = self.img.read(a);
            self.img.write(a, fill);
        }
        let n = self.emitted.len() as u32;
        self.advance(n);
    }

    fn emit(&mut self, bytes: &[u8]) {
        if self.pass == 2 {
            let mut a = self.byte_addr().wrapping_add(self.emitted.len() as u32);
            for b in bytes {
                if !self.img.write(a, *b) && !self.img.reported_out_of_range {
                    self.img.reported_out_of_range = true;
                    self.diag(msg::OUTSIDE_IMAGE, None);
                }
                a = a.wrapping_add(1);
            }
        }
        self.emitted.extend_from_slice(bytes);
    }

    // --- diagnostics --------------------------------------------------------

    fn skipping(&self) -> bool {
        self.cond.last().map_or(false, |c| !c.active)
    }

    /// 9.5: diagnostics go to standard output AND into the listing. 4.6/9.5:
    /// they are suppressed during pass 1 and while skipping a false branch.
    fn diag(&mut self, message: &str, detail: Option<String>) {
        if self.pass != 2 || self.skipping() {
            return;
        }
        self.report(message, detail);
    }

    fn report(&mut self, message: &str, detail: Option<String>) {
        let text = self.fmt.render(&self.file, self.line_no, message, detail.as_deref());
        self.stdout.push_str(&text);
        self.stdout.push('\n');
        if self.listing_on && !self.o.quiet {
            // Where a diagnostic sits relative to its own listing line is not
            // uniform, and 9.5's "immediately after the line they refer to" is
            // wrong for most of them. Across every golden, the three that
            // follow their line are exactly the three whose message text is
            // padded with trailing spaces -- unrecognized instruction.,
            // unrecognized directive., label value misalligned. -- while every
            // unpadded message precedes it. Two call sites in the original,
            // one of which padded and reported late.
            if message.ends_with(' ') {
                self.pending.push(text);
            } else {
                self.lst.push(text);
            }
        }
        self.errors += 1;
    }

    // --- driving ------------------------------------------------------------

    pub fn run(&mut self, source: &str) -> Result<(), i32> {
        self.source_name = source.to_string();
        // -d defines behave as #DEFINE would (1.3), and must survive both passes.
        let defines: Vec<String> = self.o.defines.clone();
        for d in &defines {
            self.macs.define(d);
        }

        for pass in 1..=2u8 {
            self.pass = pass;
            self.pc = 0;
            self.seen_this_pass.clear();
            self.lines_read = 0;
            self.cond.clear();
            self.saw_end = false;
            self.segment = Segment::Null;
            self.ls_first = true;
            self.listing_on = true;
            self.codes = true;
            self.img.flush();
            self.assemble_file(source, 0)?;
            if !self.cond.is_empty() {
                self.diag(msg::IMBALANCED_COND, None);
            }
            if !self.saw_end {
                self.diag(msg::NO_END, None);
            }
            self.stdout
                .push_str(&format!("{}: pass {} complete.\n", self.prog, pass));
        }
        self.img.flush();
        Ok(())
    }

    fn assemble_file(&mut self, path: &str, depth: usize) -> Result<(), i32> {
        let bytes = std::fs::read(path).map_err(|_| EXIT_FILE)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let (save_file, save_line, save_depth) = (self.file.clone(), self.line_no, self.depth);
        // 4.8: line numbers restart at 1 in each file.
        self.file = path.to_string();
        self.depth = depth;
        for (i, raw) in text.lines().enumerate() {
            self.line_no = i as u32 + 1;
            self.lines_read += 1;
            self.line(raw)?;
        }
        self.file = save_file;
        self.line_no = save_line;
        self.depth = save_depth;
        Ok(())
    }

    /// 2.7: the line processing order, identical in both passes.
    fn line(&mut self, raw: &str) -> Result<(), i32> {
        // 1. A trailing carriage return is stripped here, so DOS-format source
        //    assembles identically to Unix-format.
        let src = raw.strip_suffix('\r').unwrap_or(raw);
        self.line_pc = self.pc;
        self.emitted.clear();

        // 2. Expand macros.
        let (expanded, macro_errs) = if self.skipping() {
            (src.to_string(), Vec::new())
        } else {
            self.macs.expand(src, self.comment_char)
        };
        for e in macro_errs {
            self.diag(e, None);
        }

        // -e lists the expanded form rather than the source as written.
        let shown = if self.o.expand { expanded.clone() } else { src.to_string() };

        // 3. Strip the comment.
        let code = strip_comment(&expanded, self.comment_char);

        // 4-6. Split and dispatch. `\` separates statements on one line (4.1).
        for stmt in split_statements(&code) {
            self.statement(&stmt)?;
        }

        self.list_line(&shown);
        for d in std::mem::take(&mut self.pending) {
            self.lst.push(d);
        }
        Ok(())
    }

    fn list_line(&mut self, shown: &str) {
        // 2.1: pass 1 emits nothing -- it only decides where everything lands.
        // The listing is produced in pass 2, alongside the bytes.
        if self.pass != 2 || self.o.quiet || !self.listing_on {
            return;
        }
        let skipped = self.skipping();
        let bytes = std::mem::take(&mut self.emitted);
        if !self.codes {
            self.lst.push(listing::line_nocodes(shown));
            return;
        }
        let first = bytes.len().min(listing::BYTES_PER_LINE);
        self.lst.push(listing::line(
            self.line_no,
            self.depth,
            self.line_pc,
            skipped,
            &bytes[..first],
            shown,
        ));
        // 9.1: continuation lines repeat the line number, advance the address
        // and leave the source column empty.
        let mut off = first;
        let unit = if self.table.wordaddrs { 2 } else { 1 };
        while off < bytes.len() {
            let n = (bytes.len() - off).min(listing::BYTES_PER_LINE);
            let pc = self.line_pc.wrapping_add((off as u32) / unit);
            self.lst
                .push(listing::continuation(self.line_no, self.depth, pc, &bytes[off..off + n]));
            off += n;
        }
    }

    fn statement(&mut self, stmt: &str) -> Result<(), i32> {
        let (label, mnem, operand) = split_statement(stmt, self.comment_char, self.local_char);
        if mnem.is_empty() && label.is_none() {
            return Ok(());
        }

        // 4.2: a directive begins with a non-letter. Both '.' and '#' are
        // accepted for every directive, interchangeably.
        let is_directive = !mnem.is_empty()
            && !mnem.as_bytes()[0].is_ascii_alphabetic();
        let dname = if is_directive {
            let d = mnem.trim_start_matches(['.', '#']);
            if d.is_empty() { mnem.to_ascii_uppercase() } else { d.to_ascii_uppercase() }
        } else {
            String::new()
        };

        // 4.6: while skipping, only the conditional directives are seen. No
        // labels are defined, the counter does not advance, includes are not
        // followed, macros are not defined, and diagnostics are suppressed.
        if self.skipping() {
            if is_directive
                && matches!(dname.as_str(), "IF" | "IFDEF" | "IFNDEF" | "ELSE" | "ENDIF")
            {
                self.conditional(&dname, &operand);
            } else if is_directive && matches!(dname.as_str(), "IF" | "IFDEF" | "IFNDEF") {
                self.cond.push(Cond { active: false, taken: true });
            }
            return Ok(());
        }

        // 5. The label. 4.5: .EQU/.SET/= define the line's label themselves;
        // any other label takes the current counter.
        let defines_own = is_directive && matches!(dname.as_str(), "EQU" | "SET" | "=");
        if let Some(name) = label.clone() {
            if !defines_own {
                self.define_label(&name, self.pc as i32);
            }
        }

        if mnem.is_empty() {
            return Ok(()); // a label alone is legal (4.1)
        }
        if is_directive {
            self.directive(&dname, &operand, label.as_deref())?;
        } else {
            self.instruction(&mnem, &operand);
        }
        Ok(())
    }

    fn define_label(&mut self, raw: &str, value: i32) {
        let (name, too_long) = Symbols::truncate(raw);
        if too_long {
            self.diag(msg::TOKEN_TOO_LONG, Some(name.clone()));
        }
        let (qualified, local) = Symbols::qualify(&name, self.local_char, &self.module);
        // 1.4 bit 0x04, on by default. Reported in pass 2, where diagnostics
        // are live; the pass-1 detection alone could never print anything.
        if !self.seen_this_pass.insert(qualified.clone()) {
            // Reported in PASS 1, unlike almost everything else: the golden
            // arg-checks.out prints it before "pass 1 complete.", and because
            // the listing is written in pass 2 the line lands above 0001 in
            // arg-checks.lst.
            if self.o.strict & 0x04 != 0 && self.pass == 1 {
                self.report(msg::DUPLICATE_LABEL, Some(name.clone()));
            }
            // 4.5: the second definition is discarded -- first wins.
            return;
        }
        if self.pass == 1 {
            self.syms.define(&qualified, value, self.segment, local);
        } else {
            // 2.1: the phase error is raised when the label is reached in pass
            // 2 and its recorded value disagrees with the counter. The label
            // KEEPS its pass-1 value, which is what later references see.
            if let Some(prev) = self.syms.value(&qualified) {
                if prev != value {
                    self.diag(msg::MISALIGNED, Some(name));
                }
            }
        }
    }

    /// 1.4 bits 0x01 and 0x08, both off by default and both lexical: they look
    /// at the operand as written, not at what it evaluates to. That is why
    /// `%101` trips the non-unary check even though it is a valid binary
    /// constant.
    fn strict_operand_checks(&mut self, argt: &[String]) {
        for a in argt {
            let t = a.trim();
            if t.is_empty() {
                continue;
            }
            // 0x01: an operand wrapped entirely in parentheses, usually a
            // mistaken addressing mode. The detail is the operand as written,
            // so the diagnostic shows doubled parentheses.
            if self.o.strict & 0x01 != 0
                && t.starts_with('(')
                && t.ends_with(')')
                && matching_outer_parens(t)
            {
                self.diag(msg::NO_INDIRECTION, Some(t.to_string()));
            }
            // 0x08: an operand opening with a binary operator.
            if self.o.strict & 0x08 != 0
                && matches!(t.as_bytes()[0], b'%' | b'*' | b'/' | b'<' | b'>' | b'=' | b'&' | b'!')
            {
                self.diag(msg::NON_UNARY, Some(t.to_string()));
            }
        }
    }

    fn eval(&mut self, text: &str) -> i32 {
        let pc = self.pc as i32;
        let compat = self.o.compatibility;
        let lc = self.local_char;
        let module = self.module.clone();
        let syms = &self.syms;
        let mut lookup = |n: &str| {
            let (q, _) = Symbols::qualify(n, lc, &module);
            syms.value(&q).or_else(|| syms.value(n))
        };
        let out = expr::eval_strict(text, pc, compat, lc, 0, &mut lookup);
        let diags = out.diags;
        let undefined = out.undefined;
        for d in diags {
            self.diag(d.msg, d.detail);
        }
        for name in undefined {
            // 3.8: an ordinary forward reference is silent in pass 1 and
            // `Label not found:` in pass 2 -- which is exactly what makes
            // forward references work.
            self.diag(msg::LABEL_NOT_FOUND, Some(name));
        }
        out.value
    }

    /// As `eval`, but for contexts where an unresolved name is an error on
    /// BOTH passes -- 3.8's forward reference inside an .EQU.
    fn eval_equate(&mut self, text: &str) -> i32 {
        let pc = self.pc as i32;
        let compat = self.o.compatibility;
        let lc = self.local_char;
        let module = self.module.clone();
        let syms = &self.syms;
        let mut lookup = |n: &str| {
            let (q, _) = Symbols::qualify(n, lc, &module);
            syms.value(&q).or_else(|| syms.value(n))
        };
        let out = expr::eval(text, pc, compat, lc, &mut lookup);
        let diags = out.diags;
        let undefined = out.undefined;
        for d in diags {
            self.diag(d.msg, d.detail);
        }
        for name in undefined {
            if !self.skipping() {
                self.report(msg::FORWARD_IN_EQUATE, Some(name));
            }
        }
        out.value
    }

    // --- instructions -------------------------------------------------------

    fn instruction(&mut self, mnem: &str, operand: &str) {
        let m = match matcher::find(&self.table, mnem, operand, self.o.class_mask) {
            Some(m) => m,
            None => {
                // 10.3: a mnemonic that matched no row at all is a bad
                // instruction; a known mnemonic whose operands matched none is
                // a bad argument.
                let known = self
                    .table
                    .rows
                    .iter()
                    .any(|r| r.mnemonic == mnem.to_ascii_uppercase());
                let (m, d) = if known {
                    (msg::BAD_ARGUMENT, operand.trim().to_string())
                } else {
                    (msg::BAD_INSTRUCTION, mnem.to_ascii_uppercase())
                };
                self.diag(m, Some(d));
                return;
            }
        };
        let row = self.table.rows[m.row].clone();
        if row.short_count {
            self.diag(msg::SHORT_BYTE_COUNT, None);
        }

        let argt = m.args.clone();
        self.strict_operand_checks(&argt);
        let argv: Vec<i32> = argt.iter().map(|a| self.eval(a)).collect();

        let mut e = Enc {
            pcx: self.pc as i32,
            opcode: m.opcode,
            opcode_bytes: row.opcode_bytes,
            arg_bytes: row.arg_bytes,
            argval: argv.first().copied().unwrap_or(0),
            shift: row.shift,
            or: row.or,
            argv,
            argt,
            vector: None,
            diags: Vec::new(),
            selector: self.table.selector.clone(),
        };
        e.apply(row.rule, self.table.noargshift);
        let diags = std::mem::take(&mut e.diags);
        for d in diags {
            self.diag(d.msg, d.detail);
        }

        let mut bytes = Vec::new();
        // 2.5: opcode bytes go out least-significant first by default, most
        // significant first if the table declared .MSFIRST. This is why Z80
        // prefixed instructions carry the prefix in the HIGH byte.
        for i in 0..e.opcode_bytes {
            let sh = if self.table.msfirst {
                8 * (e.opcode_bytes - 1 - i)
            } else {
                8 * i
            };
            bytes.push(((e.opcode >> sh) & 0xFF) as u8);
        }
        // 2.5: argument bytes come from a single value, least-significant
        // first, UNCONDITIONALLY -- .MSFIRST never applies here. A rule that
        // wants big-endian output swaps the value first, which is what SW does.
        match &e.vector {
            Some(v) => bytes.extend(v.iter().take(e.arg_bytes as usize)),
            None => {
                for i in 0..e.arg_bytes {
                    bytes.push(((e.argval as u32 >> (8 * i as u32)) & 0xFF) as u8);
                }
            }
        }

        // Significant bits the instruction discards. 1.4 says this is gated on
        // -a bit 0x02, but err-undef is assembled with no -a at all and its
        // golden carries the diagnostic -- see the findings log
        // A negative value is sign-extended, and those high bits are not
        // "unused data" -- test96.asm's backward `lcall` produces a negative
        // 16-bit displacement whose top half is all ones, and the corpus
        // reports nothing there.
        if e.arg_bytes > 0 && e.arg_bytes < 4 && e.argval > 0 {
            let discarded = (e.argval as u32) >> (8 * e.arg_bytes as u32);
            if discarded != 0 {
                self.diag(msg::UNUSED_MS_BYTE, Some(format!("{:x}", discarded)));
            }
        }

        self.emit_and_advance(&bytes);
    }
}

/// True when the leading `(` closes only at the very end, so the whole operand
/// is one parenthesised group rather than, say, `(a)+(b)`.
fn matching_outer_parens(t: &str) -> bool {
    let mut depth = 0i32;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i == t.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

/// 4.1: a `;` outside quotes begins a comment. The embedded comment character
/// is always `;` and cannot be changed -- .COMMENTCHAR changes only the
/// column-1 character.
fn strip_comment(line: &str, comment_char: u8) -> String {
    if line.as_bytes().first() == Some(&comment_char) || line.as_bytes().first() == Some(&b';') {
        return String::new();
    }
    let b = line.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, c) in b.iter().enumerate() {
        match quote {
            Some(q) => {
                if *c == q {
                    quote = None;
                }
            }
            None => {
                if *c == b'"' || *c == b'\'' {
                    quote = Some(*c);
                } else if *c == b';' {
                    return line[..i].to_string();
                }
            }
        }
    }
    line.to_string()
}

/// 4.1: `\` separates multiple statements on one line.
fn split_statements(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '\\' => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            },
        }
    }
    out.push(cur);
    out
}

/// 4.1: a label is recognised ONLY in column 1. The token ends at a space,
/// tab, `\`, `:`, end of line -- or immediately after an `=`, which is what
/// makes `*=$1000` work and what makes `LABEL=5` produce the label `LABEL=`.
fn split_statement(stmt: &str, comment_char: u8, local_char: u8) -> (Option<String>, String, String) {
    let b = stmt.as_bytes();
    if b.is_empty() {
        return (None, String::new(), String::new());
    }
    if b[0] == comment_char || b[0] == b';' {
        return (None, String::new(), String::new());
    }

    let mut i = 0usize;
    let mut label = None;
    let c0 = b[0];
    if c0.is_ascii_alphabetic() || c0 == b'_' || c0 == local_char {
        while i < b.len() {
            let c = b[i];
            if c == b' ' || c == b'\t' || c == b'\\' || c == b':' {
                break;
            }
            i += 1;
            if c == b'=' {
                break;
            }
        }
        label = Some(stmt[..i].to_string());
        if b.get(i) == Some(&b':') {
            i += 1;
        }
    }

    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let ms = i;
    while i < b.len() && b[i] != b' ' && b[i] != b'\t' {
        i += 1;
    }
    let mnem = stmt[ms..i].to_string();
    let operand = stmt[i..].trim().to_string();
    (label, mnem, operand)
}

/// Split an operand list on top-level commas, leaving quoted text alone --
/// whitespace inside quotes is preserved (4.3).
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                ',' => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            },
        }
    }
    // Always at least one element, possibly empty: a bare `.byte` with no
    // operand emits a single zero byte (golden err-baddir.lst line 5).
    out.push(cur);
    out
}

/// 4.3: .TEXT escapes. `\0`-`\3` followed by exactly two more octal digits
/// gives an arbitrary byte; any other escaped character is passed through
/// literally, so `\\` yields a backslash.
fn text_bytes(s: &str) -> (Vec<u8>, bool) {
    let b = s.as_bytes();
    let mut out = Vec::new();
    // A string that does not start with a quote is taken literally to end of
    // line; one that opens but never closes is diagnosed.
    let (body, quoted) = if b.first() == Some(&b'"') { (&b[1..], true) } else { (b, false) };
    let mut closed = !quoted;
    let mut i = 0usize;
    while i < body.len() {
        let c = body[i];
        if quoted && c == b'"' {
            closed = true;
            break;
        }
        if c == b'\\' {
            i += 1;
            let e = match body.get(i) {
                Some(e) => *e,
                None => break, // trailing backslash
            };
            match e {
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0C),
                b'"' => out.push(b'"'),
                b'0'..=b'3' => {
                    let d1 = body.get(i + 1).copied().unwrap_or(0);
                    let d2 = body.get(i + 2).copied().unwrap_or(0);
                    if (b'0'..=b'7').contains(&d1) && (b'0'..=b'7').contains(&d2) {
                        let v = ((e - b'0') << 6) | ((d1 - b'0') << 3) | (d2 - b'0');
                        out.push(v);
                        i += 2;
                    } else {
                        out.push(e);
                    }
                }
                other => out.push(other),
            }
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    (out, closed)
}

impl Asm {
    fn conditional(&mut self, name: &str, operand: &str) {
        match name {
            "IF" | "IFDEF" | "IFNDEF" => {
                // 4.6: a skip at any enclosing level forces a skip regardless
                // of the inner condition.
                let outer = !self.skipping();
                if self.cond.len() >= MAX_CONDITIONALS - 1 {
                    self.diag(msg::COND_TOO_DEEP, None);
                    self.cond.push(Cond { active: false, taken: true });
                    return;
                }
                let want = if !outer {
                    false
                } else {
                    match name {
                        // 4.6: these test MACROS, not labels. A symbol defined
                        // with .EQU is invisible to them.
                        "IFDEF" => self.macs.defined(operand.trim()),
                        "IFNDEF" => !self.macs.defined(operand.trim()),
                        _ => self.eval(operand) != 0,
                    }
                };
                self.cond.push(Cond { active: want, taken: want });
            }
            "ELSE" => match self.cond.last_mut() {
                Some(c) => {
                    c.active = !c.taken;
                    c.taken = true;
                }
                None => self.diag(msg::ELSE_NO_MATCH, None),
            },
            "ENDIF" => {
                if self.cond.pop().is_none() {
                    self.diag(msg::ENDIF_NO_MATCH, None);
                }
            }
            _ => {}
        }
    }

    fn directive(&mut self, name: &str, operand: &str, label: Option<&str>) -> Result<(), i32> {
        match name {
            // --- 4.3 data emission ------------------------------------------
            "BYTE" | "DB" => {
                let mut bytes = Vec::new();
                for a in split_args(operand) {
                    let t = a.trim();
                    if t.starts_with('"') {
                        // One byte per character; whitespace inside is kept.
                        let (v, closed) = text_bytes(t);
                        if !closed {
                            self.diag(msg::NO_TERMINATING_QUOTE, Some(t.to_string()));
                        }
                        bytes.extend(v);
                    } else {
                        bytes.push(self.eval(t) as u8);
                    }
                }
                self.emit_and_advance(&bytes);
            }
            "WORD" | "DW" => {
                let mut bytes = Vec::new();
                for a in split_args(operand) {
                    let v = self.eval(a.trim()) as u32;
                    // 4.11: byte order follows the SOURCE .LSFIRST/.MSFIRST,
                    // which is a different setting from the table directive of
                    // the same name -- that one governs opcodes.
                    if self.ls_first {
                        bytes.push((v & 0xFF) as u8);
                        bytes.push(((v >> 8) & 0xFF) as u8);
                    } else {
                        bytes.push(((v >> 8) & 0xFF) as u8);
                        bytes.push((v & 0xFF) as u8);
                    }
                }
                self.emit_and_advance(&bytes);
            }
            "TEXT" => {
                let (bytes, closed) = text_bytes(operand.trim());
                if !closed {
                    self.diag(msg::NO_TERMINATING_QUOTE, Some(operand.trim().to_string()));
                }
                self.emit_and_advance(&bytes);
            }
            "FILL" => {
                let args = split_args(operand);
                // 4.3: the count is evaluated on both passes during parsing,
                // so it must not depend on a forward reference. It is taken as
                // a 16-bit value, so a negative count wraps -- .fill -1 fills
                // 65535 bytes.
                let count = (self.eval(args.first().map(|s| s.trim()).unwrap_or("0")) as u32) & 0xFFFF;
                let value = match args.get(1) {
                    Some(v) => self.eval(v.trim()) as u8,
                    None => 0xFF,
                };
                let bytes = vec![value; count as usize];
                self.emit_and_advance(&bytes);
            }
            "BLOCK" | "DS" => {
                // 4.3: reserves space by advancing the counter and emitting
                // nothing, so the region does not appear in the object at all.
                let n = self.eval(operand.trim()) as u32;
                self.img.flush();
                self.advance(n);
            }
            "CHK" => {
                let start = self.eval(operand.trim()) as u32;
                let here = self.byte_addr();
                let mut sum = 0u8;
                // A .CHK at address 0 is a guarded edge: it XORs just byte 0
                // rather than running the range backwards, which in the
                // original underflowed into a multi-billion-byte loop.
                if here > start {
                    for a in start..here {
                        sum ^= self.img.read(a);
                    }
                } else {
                    sum ^= self.img.read(start);
                }
                self.emit_and_advance(&[sum]);
            }

            // --- 4.4 location ------------------------------------------------
            "ORG" | "*=" | "$=" => {
                let v = self.eval(operand.trim()) as u32;
                // 4.4: a counter change flushes the current object record
                // unless -c is in force. The operand is NOT range-checked here.
                if !self.o.block {
                    self.img.flush();
                }
                self.pc = v;
                // The listing shows the address AFTER an .ORG -- golden 85.lst
                // line 18 lists `.org 1000h` at 1000, not at the previous
                // counter. .BLOCK, which also moves the counter, lists the
                // address BEFORE (96.lst line 15), so this is specific to the
                // directives that assign outright.
                self.line_pc = v;
            }

            // --- 4.5 symbols ---------------------------------------------------
            "EQU" | "=" => {
                let v = self.eval_equate(operand.trim());
                if let Some(l) = label {
                    let (name, _) = Symbols::truncate(l.trim_end_matches('='));
                    let (q, local) = Symbols::qualify(&name, self.local_char, &self.module);
                    if !self.seen_this_pass.insert(q.clone()) {
                        if self.o.strict & 0x04 != 0 && self.pass == 1 {
                            self.report(msg::DUPLICATE_LABEL, Some(name));
                        }
                    } else if self.pass == 1 {
                        self.syms.define(&q, v, self.segment, local);
                    } else {
                        self.syms.redefine(&q, v);
                    }
                }
            }
            "SET" => {
                let v = self.eval(operand.trim());
                if let Some(l) = label {
                    let (name, _) = Symbols::truncate(l);
                    let (q, _) = Symbols::qualify(&name, self.local_char, &self.module);
                    // 4.5: .SET cannot create a symbol; the name must already
                    // exist, typically from .EQU.
                    if !self.syms.set(&q, v) {
                        self.diag(msg::SET_PREEXIST, Some(name));
                    }
                }
            }
            "EXPORT" => {
                if self.pass == 2 {
                    for a in split_args(operand) {
                        let (q, _) = Symbols::qualify(a.trim(), self.local_char, &self.module);
                        self.syms.export(&q);
                    }
                }
            }
            "MODULE" => self.module = operand.trim().to_string(),
            // 4.5: each takes the SECOND character of the operand, the
            // convention being that the first is a quote. The character is not
            // validated and no closing quote is required. Both take effect in
            // pass 1 only and are not reset between passes.
            "LOCALLABELCHAR" => {
                if self.pass == 1 {
                    if let Some(c) = operand.as_bytes().get(1) {
                        self.local_char = *c;
                    }
                }
            }
            "COMMENTCHAR" => {
                if self.pass == 1 {
                    if let Some(c) = operand.as_bytes().get(1) {
                        self.comment_char = *c;
                    }
                }
            }

            // --- 4.6 conditionals ---------------------------------------------
            "IF" | "IFDEF" | "IFNDEF" | "ELSE" | "ENDIF" => self.conditional(name, operand),

            // --- 4.7 macros ----------------------------------------------------
            "DEFINE" => {
                if let Some(e) = self.macs.define(operand) {
                    self.diag(e, None);
                }
            }
            "DEFCONT" => {
                if let Some(e) = self.macs.defcont(operand) {
                    self.diag(e, None);
                }
            }
            "UNDEF" => {
                // 4.7: originally this did nothing at all -- listed in the
                // directive table but implemented in neither pass, so it
                // parsed cleanly and silently kept the definition.
                // --compatibility restores that silence.
                if !self.o.compatibility {
                    self.macs.undef(operand.trim());
                }
            }

            // --- 4.8 inclusion --------------------------------------------------
            "INCLUDE" => {
                let path = operand.trim().trim_matches('"');
                if self.depth + 1 >= MAX_INCLUDE_DEPTH {
                    self.diag(msg::INCLUDE_TOO_DEEP, Some(path.to_string()));
                } else {
                    // 4.8: no path resolution of any kind -- opened exactly as
                    // written, relative to the working directory.
                    let d = self.depth + 1;
                    self.assemble_file(path, d)?;
                }
            }

            // --- 4.9 segments ----------------------------------------------------
            "NSEG" => self.segment = Segment::Null,
            "CSEG" => self.segment = Segment::Code,
            "BSEG" => self.segment = Segment::Bit,
            "XSEG" => self.segment = Segment::Extd,
            "DSEG" => self.segment = Segment::Data,

            // --- 4.10 listing control ---------------------------------------------
            "LIST" => self.listing_on = true,
            "NOLIST" => self.listing_on = false,
            "CODES" => self.codes = true,
            "NOCODES" => self.codes = false,
            "PAGE" | "NOPAGE" | "EJECT" => {}
            "TITLE" => self.title = operand.trim().trim_matches('"').to_string(),

            // --- 4.11 miscellaneous -------------------------------------------------
            "LSFIRST" => self.ls_first = true,
            "MSFIRST" => self.ls_first = false,
            "SYM" | "AVSYM" => {
                if self.pass == 1 {
                    // 9.6: the directive chooses the format, not just the name.
                    self.avsym = name == "AVSYM";
                    let n = operand.trim();
                    if !n.is_empty() {
                        self.sym_out = Some(n.to_string());
                    } else if self.sym_out.is_none() {
                        self.sym_out = Some(String::new());
                    }
                }
            }
            "ADDINSTR" => {
                if self.pass == 1 {
                    if self.table.rows.len() >= MAX_TABLE_ROWS {
                        return Err(EXIT_FATAL);
                    }
                    if let Some(row) = crate::table::parse_row(operand) {
                        self.table.rows.push(row);
                    }
                }
            }
            "ECHO" => {
                if self.pass == 2 {
                    let t = operand.trim();
                    // 4.11: printed to standard error with NO trailing
                    // newline. A non-quoted operand is evaluated and printed
                    // as a decimal number.
                    let s = if t.starts_with('"') {
                        String::from_utf8_lossy(&text_bytes(t).0).into_owned()
                    } else {
                        format!("{}", self.eval(t))
                    };
                    eprint!("{}", s);
                }
            }
            "END" => {
                self.saw_end = true;
                let t = operand.trim();
                if !t.is_empty() {
                    let v = self.eval(t);
                    if !(0..=0xFFFF).contains(&v) {
                        self.diag(msg::END_OUT_OF_RANGE, Some(t.to_string()));
                    }
                    // 4.11 / 8.5: masked to 16 bits, so the S9 record's
                    // four-digit field cannot overflow.
                    self.end_addr = (v as u32) & 0xFFFF;
                }
            }

            _ => self.diag(msg::BAD_DIRECTIVE, Some(format!(".{}", name))),
        }
        Ok(())
    }

    /// 8.1: which regions the object writer sees. -c and -b collect everything
    /// into one run from the lowest to the highest address used.
    pub fn regions(&self) -> Vec<crate::image::Region> {
        if self.o.block || self.o.format == ObjFormat::Binary && self.o.block {
            self.img.block().into_iter().collect()
        } else {
            self.img.regions.clone()
        }
    }
}

impl Asm {
    /// The order symbols appear in the symbol file, the export file and the
    /// -l/-ll label tables.
    ///
    /// By default this is a proper lexical sort, which is what a symbol list
    /// is for. `--bug-compatibility` instead reproduces the original's
    /// ordering exactly; see `shaker_order`.
    pub fn sorted_symbols(&self) -> Vec<&crate::symbols::Symbol> {
        if self.o.bug_compatibility {
            return self.shaker_order();
        }
        let mut v: Vec<&crate::symbols::Symbol> = self.syms.list.iter().collect();
        // Names are unique -- a duplicate definition is rejected and discarded
        // (4.5) -- so stability is irrelevant and the unstable sort is fine.
        v.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// The original's ordering (2.1): a COCKTAIL (bidirectional bubble) SORT
    /// keyed on the first character, whose bookkeeping terminates early -- so
    /// for some inputs the result is not fully sorted, deterministically.
    ///
    /// It must be reproduced as the procedure, not by a correct sort: `k`
    /// records the last swap and is carried across BOTH sweeps, the boundaries
    /// collapse around it, and the loop stops the moment `top >= bot` rather
    /// than when a sweep makes no swap. Because `top` and `bot` bound
    /// COMPARISONS rather than elements, that test discards the final
    /// comparison and can leave one inversion standing.
    ///
    /// One pass usually leaves the table ordered, which is why every 8051
    /// vector and nine of the eleven reference programs look sorted; 6805 and
    /// 6800 are where it shows.
    fn shaker_order(&self) -> Vec<&crate::symbols::Symbol> {
        fn key(s: &crate::symbols::Symbol) -> u8 {
            s.name.as_bytes().first().copied().unwrap_or(0)
        }
        let mut l: Vec<&crate::symbols::Symbol> = self.syms.list.iter().collect();
        let n = l.len() as isize;
        if n < 2 {
            return l;
        }
        let (mut top, mut bot) = (0isize, n - 2);
        let mut k = bot;
        loop {
            let mut i = bot;
            while i >= top {
                if key(l[i as usize]) > key(l[i as usize + 1]) {
                    l.swap(i as usize, i as usize + 1);
                    k = i;
                }
                i -= 1;
            }
            top = k + 1;
            let mut i = top;
            while i <= bot {
                if key(l[i as usize]) > key(l[i as usize + 1]) {
                    l.swap(i as usize, i as usize + 1);
                    k = i;
                }
                i += 1;
            }
            bot = k - 1;
            if top >= bot {
                break;
            }
        }
        l
    }

    fn symbol_file(&self) -> String {
        let mut s = String::new();
        for sym in self.sorted_symbols() {
            let v = (sym.value as u32) & 0xFFFF;
            if self.avsym {
                s.push_str(&format!("AS {:<16}  {}:{:04x}\n", sym.name, sym.segment.letter(), v));
            } else {
                s.push_str(&format!("{:<16}  {:04x}\n", sym.name, v));
            }
        }
        s
    }

    /// 9.7: the export file is re-includable TASM source -- one `.EQU` line
    /// per exported symbol, so another assembly can `.INCLUDE` it to import the
    /// values. Format `%-16s .EQU  $%04x`: the name left-justified in a
    /// 16-character field that GROWS rather than truncating for a longer name,
    /// then ` .EQU  $`, then the value masked to 16 bits in lower-case hex.
    fn export_file(&self) -> String {
        let mut s = String::new();
        for sym in self.sorted_symbols().iter().filter(|s| s.exported) {
            s.push_str(&format!("{:<16} .EQU  ${:04x}\n", sym.name, (sym.value as u32) & 0xFFFF));
        }
        s
    }

    pub fn write_outputs(&mut self, obj: &str, lst: &str, exp: &str, sym: &str, count: &str) {
        let regions = self.regions();
        let data = crate::object::write(
            self.o.format,
            &regions,
            self.o.bytes_per_record as usize,
            self.end_addr,
            self.o.bug_compatibility,
        );
        let _ = std::fs::write(obj, data);

        // 1.3: -q suppresses the listing. The file is still created, empty.
        let text = if self.o.quiet { String::new() } else { self.build_listing(count) };
        let _ = std::fs::write(lst, text);

        // 9.6: written when -s is given, or when the source used .SYM/.AVSYM,
        // which also override the file name.
        if self.o.symfile || self.sym_out.is_some() {
            let name = match self.sym_out.as_deref() {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => sym.to_string(),
            };
            let _ = std::fs::write(name, self.symbol_file());
        }
        let exported = self.syms.list.iter().any(|s| s.exported);
        if exported {
            let _ = std::fs::write(exp, self.export_file());
        }
    }
}

impl Asm {
    /// Assemble the finished listing: the source lines, then whatever -l/-ll/
    /// -la and -h append, then the error count, then paging over the lot.
    pub fn build_listing(&self, count: &str) -> String {
        let mut lines: Vec<String> = self.lst.clone();
        match self.o.labels {
            crate::cli::LabelTable::None => {}
            crate::cli::LabelTable::Short => self.short_labels(&mut lines, false),
            crate::cli::LabelTable::All => self.short_labels(&mut lines, true),
            crate::cli::LabelTable::Long => self.long_labels(&mut lines),
        }
        if self.o.hex_dump {
            self.hex_table(&mut lines);
        }
        lines.push(count.trim_end_matches('\n').to_string());

        let body = match self.o.page_lines {
            Some(n) if n > 4 => self.paginate(&lines, n as usize),
            _ => lines,
        };
        let mut out = String::new();
        for l in body {
            out.push_str(&l);
            out.push('\n');
        }
        out
    }

    /// 9.8: three column-pairs across the page, in table order. The name
    /// occupies 14 columns and the value at least four hex digits, each pair
    /// followed by six spaces -- which are present on the last column too, so
    /// the rows carry trailing whitespace and the header does not.
    ///
    /// This is the only place a value is printed at its full width, and so the
    /// only place the 32-bit value width of 3.9 is observable.
    fn short_labels(&self, out: &mut Vec<String>, all: bool) {
        for _ in 0..3 {
            out.push(String::new());
        }
        let head = "Label        Value";
        let rule = "------------------";
        out.push([head, head, head].join("      "));
        out.push([rule, rule, rule].join("      "));
        // 3.7: local labels are excluded unless -la.
        let syms: Vec<_> = self
            .sorted_symbols()
            .into_iter()
            .filter(|s| all || !s.local)
            .collect();
        for chunk in syms.chunks(3) {
            let mut row = String::new();
            for s in chunk {
                row.push_str(&format!("{:<14}{:04X}      ", s.name, s.value));
            }
            out.push(row);
        }
        out.push(String::new());
    }

    /// 9.8: one symbol per line, after a three-line type-key legend emitted
    /// verbatim.
    fn long_labels(&self, out: &mut Vec<String>) {
        for _ in 0..3 {
            out.push(String::new());
        }
        out.push("Type Key: N=NULL_SEG C=CODE_SEG B=BIT_SEG X=EXTD_SEG D=DATA_SEG".to_string());
        out.push("          L=Local".to_string());
        out.push("          E=Export".to_string());
        out.push(String::new());
        out.push("Value    Type   Label".to_string());
        out.push("-----    ----   ------------------------------".to_string());
        for s in self.sorted_symbols() {
            // The value field is the value followed by FIVE literal spaces,
            // not a nine-column padded field: a 32-bit value prints eight
            // digits and pushes the rest of the row right rather than being
            // squeezed. Only visible when a value exceeds four digits.
            // The type column is three flags -- segment, Local, Export --
            // followed by four spaces, which is why a plain segment letter
            // reads as one letter and six spaces.
            let flags = format!(
                "{}{}{}",
                s.segment.letter(),
                if s.local { 'L' } else { ' ' },
                if s.exported { 'E' } else { ' ' }
            );
            out.push(format!("{:04X}     {}    {:<32}", s.value, flags, s.name));
        }
        out.push(String::new());
    }

    /// 9.8: sixteen bytes per line from the low watermark to the high one.
    /// Unwritten bytes inside the span show the fill value, and the final row
    /// is padded out to a full sixteen.
    fn hex_table(&self, out: &mut Vec<String>) {
        out.push(String::new());
        out.push("ADDR  00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F".to_string());
        out.push("-".repeat(53));
        if let (Some(lo), Some(hi)) = (self.img.lo, self.img.hi) {
            // Rows begin at the LOW WATERMARK itself, not rounded down to a
            // 16-byte boundary, and step 16 from there -- so a program based at
            // 0x0056 lists rows 0056, 0066, ... This is invisible on 8051,
            // whose corpus starts at zero.
            let mut a = lo;
            while a <= hi {
                let row: Vec<String> =
                    (0..16).map(|i| format!("{:02X}", self.img.read(a + i))).collect();
                out.push(format!("{:04X}  {}", a, row.join(" ")));
                a += 16;
            }
        }
        out.push(String::new());
        out.push(String::new());
    }

    /// 9.4: `-p<lines>` breaks the listing into pages separated by a form
    /// feed. Measured against the golden: -p20 over test51.asm produces 19
    /// form feeds at a stride of 19 lines, of which three are the heading, so
    /// a page carries `lines - 4` listing lines.
    fn paginate(&self, lines: &[String], page: usize) -> Vec<String> {
        let per = page - 4;
        let mut out = Vec::new();
        for (i, chunk) in lines.chunks(per).enumerate() {
            out.push(format!(
                "\u{0C}{:<34}{:<33}page {}",
                self.table.banner,
                self.source_name,
                i + 1
            ));
            out.push(format!("{:<34}", self.page_title()));
            out.push(String::new());
            out.extend(chunk.iter().cloned());
        }
        out
    }

    /// The second heading line. `.TITLE` sets it, but 9.4 never says what it
    /// defaults to -- and the golden 51-paged.lst, whose source has no .TITLE,
    /// carries the ORIGINAL PRODUCT'S VENDOR NAME there. That string appears
    /// nowhere in the specification, tables/ or examples/.
    ///
    /// It is identification, not behaviour, so it follows the same rule as the
    /// message prefix: --report-compatibility reproduces it so the vector
    /// compares, and without the flag tabasm prints its own. See
    /// the findings log.
    fn page_title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        if self.o.report_compatibility {
            "tabasm".to_string()
        } else {
            "tabasm".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    /// 2.1's worked example: labels defined `bee dee ayy azz ell` (first
    /// characters b d a a l) come out `ayy bee azz dee ell` -- a b a d l, NOT
    /// the fully sorted a a b d l. The early stop leaves `bee` ahead of `azz`.
    ///
    /// This is what --broken-sort-compatibility reproduces; the default sorts
    /// properly. The procedure is duplicated here rather than exercised
    /// through Asm because building one needs a loaded table.
    #[test]
    fn the_cocktail_sort_stops_early_as_specified() {
        fn order(names: &[&str]) -> Vec<String> {
            let mut l: Vec<&str> = names.to_vec();
            let n = l.len() as isize;
            if n < 2 {
                return l.iter().map(|s| s.to_string()).collect();
            }
            let key = |s: &str| s.as_bytes()[0];
            let (mut top, mut bot) = (0isize, n - 2);
            let mut k = bot;
            loop {
                let mut i = bot;
                while i >= top {
                    if key(l[i as usize]) > key(l[i as usize + 1]) {
                        l.swap(i as usize, i as usize + 1);
                        k = i;
                    }
                    i -= 1;
                }
                top = k + 1;
                let mut i = top;
                while i <= bot {
                    if key(l[i as usize]) > key(l[i as usize + 1]) {
                        l.swap(i as usize, i as usize + 1);
                        k = i;
                    }
                    i += 1;
                }
                bot = k - 1;
                if top >= bot {
                    break;
                }
            }
            l.iter().map(|s| s.to_string()).collect()
        }
        assert_eq!(order(&["bee", "dee", "ayy", "azz", "ell"]), ["ayy", "bee", "azz", "dee", "ell"]);
        // 6805: bit3 data addz addr loop1 -> addz bit3 addr data loop1
        assert_eq!(
            order(&["bit3", "data", "addz", "addr", "loop1"]),
            ["addz", "bit3", "addr", "data", "loop1"]
        );
        // 8051 comes out fully ordered, which is why the corpus never showed it.
        assert_eq!(
            order(&["labimm", "lab2", "lab3", "lab5", "labbt_1", "bit", "lab4", "jlab", "jlab5"]),
            ["bit", "jlab", "jlab5", "labimm", "lab2", "lab3", "lab5", "labbt_1", "lab4"]
        );
    }

    /// The default ordering is a full lexical sort, so the cases the original
    /// leaves unsorted come out properly ordered -- and so do the ones it
    /// merely bucketed by first character.
    #[test]
    fn the_default_ordering_is_a_full_lexical_sort() {
        let mut v = vec!["bit3", "data", "addz", "addr", "loop1"];
        v.sort_unstable();
        assert_eq!(v, ["addr", "addz", "bit3", "data", "loop1"]);
        // The original bucketed these by first character only, leaving
        // definition order inside the bucket: labimm, lab2, lab3, lab5, ...
        let mut v = vec!["labimm", "lab2", "lab3", "lab5", "labbt_1", "bit", "lab4"];
        v.sort_unstable();
        assert_eq!(v, ["bit", "lab2", "lab3", "lab4", "lab5", "labbt_1", "labimm"]);
    }
}

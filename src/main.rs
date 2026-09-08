//! tabasm -- a clean-room reimplementation of the TASM cross-assembler.
//!
//! Built from the specification alone. See AGENTS.md: no part of this descends from the
//! original implementation's source or binaries.

#![allow(dead_code)] // constants and helpers land ahead of their first use

mod asm;
mod cli;
mod errlog;
mod expr;
mod image;
mod matcher;
mod object;
mod rules;
mod symbols;
mod table;
mod limits;
mod listing;
mod macros;

use cli::Options;
use limits::*;
use std::io::Write;

/// The prefix on the assembler's own messages -- the ones that are not
/// per-line diagnostics: the progress lines, the error count, and the fatal
/// reports.
///
/// The corpus in tests/golden/ was produced by a program called `tasm` and
/// pins these lines byte-for-byte inside the compared output. This binary is
/// called `tabasm`, so the prefix follows the binary by default and
/// --report-compatibility restores the old spelling for comparison.
///
/// Note this covers stderr as well as stdout: the golden .err files for
/// errfmt-mismatch and errfmt-percent-n contain
///     tasm: ignoring TASMERRFORMAT: unsupported conversion ...
/// and are compared byte-for-byte, despite the harness's comment claiming
/// every .err is empty.
///
/// Every such message must route through here rather than embedding a literal.
pub fn prog(o: &Options) -> &'static str {
    if o.report_compatibility {
        "tasm"
    } else {
        "tabasm"
    }
}

/// The acceptance criteria: the two-line identification banner is the one
/// thing excluded from the standard-output comparison, precisely so that a
/// clean-room reimplementation prints its own identification instead of the
/// original's. Keep it to two lines; the harness skips exactly that many.
///
/// This deliberately does NOT follow --report-compatibility. That flag exists
/// to keep compared output byte-identical, and the banner is the one thing
/// never compared -- reverting it would mean printing the original product's
/// name and author, which is the opposite of what Acceptance asks for.
fn banner(out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "tabasm {} -- table-driven cross-assembler.",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(out, " Clean-room reimplementation from the TASM behavioural specification.");
}

fn main() {
    let started = std::time::Instant::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let tasmopts = std::env::var("TASMOPTS").ok();
    let parsed = Options::parse(&argv, tasmopts.as_deref());
    let o = parsed.opts;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    banner(&mut out);
    for w in &parsed.warnings {
        let _ = writeln!(out, "{}: {}", prog(&o), w);
    }

    if o.source().is_none() {
        let _ = writeln!(out, "{}: no source file", prog(&o));
        let _ = out.flush();
        std::process::exit(EXIT_FILE);
    }

    // 9.5: a rejected TASMERRFORMAT warns once, on standard error, and the
    // built-in layout is used instead.
    let (fmt, warn) = errlog::Format::from_env(std::env::var("TASMERRFORMAT").ok().as_deref());
    if let Some(w) = warn {
        eprintln!("{}: {}", prog(&o), w);
    }

    // 1.5: failure to open the table is fatal, exit 3.
    let table = match o.table_path(std::env::var("TASMTABS").ok().as_deref()) {
        Some(path) => match table::Table::load(&path, o.table.as_deref().unwrap_or("")) {
            Ok(t) => t,
            Err(table::LoadError::Open(p)) => {
                let _ = writeln!(out, "{}: cannot open table file {}", prog(&o), p);
                let _ = out.flush();
                std::process::exit(EXIT_FILE);
            }
            Err(table::LoadError::TooManyRows) => {
                let _ = writeln!(out, "{}: Max number of instructions exceeded", prog(&o));
                let _ = out.flush();
                std::process::exit(EXIT_FATAL);
            }
            Err(table::LoadError::TooManyRegsets) => {
                let _ = writeln!(out, "{}: Max number of registers exceeded", prog(&o));
                let _ = out.flush();
                std::process::exit(EXIT_FATAL);
            }
        },
        None => {
            let _ = writeln!(out, "{}: no instruction table selected", prog(&o));
            let _ = out.flush();
            std::process::exit(EXIT_FILE);
        }
    };

    let source = o.source().unwrap().to_string();
    let (base, base_warn) = o.base_name();
    if let Some(w) = base_warn {
        let _ = writeln!(out, "{}: {}", prog(&o), w);
    }
    let obj_name = o.out_name(1, ".obj", &base);
    let lst_name = o.out_name(2, ".lst", &base);
    let exp_name = o.out_name(3, ".exp", &base);
    let sym_name = o.out_name(4, ".sym", &base);

    let p = prog(&o);
    let mut a = asm::Asm::new(o, table, fmt, p);
    if let Err(code) = a.run(&source) {
        let _ = write!(out, "{}", a.stdout);
        let _ = writeln!(out, "{}: file access failure", p);
        let _ = out.flush();
        std::process::exit(code);
    }

    let _ = write!(out, "{}", a.stdout);
    // 1.3: -y reports elapsed time on standard output. The figures are
    // machine-dependent, so Acceptance excludes the line the way it excludes
    // the identity strings; a reimplementation need not match them exactly.
    if a.o.timing {
        let secs = started.elapsed().as_secs_f64();
        let rate = if secs > 0.0 { (a.lines_read as f64 / secs) as u64 } else { 0 };
        let _ = writeln!(
            out,
            "Elapsed time = {:.2} secs  lines = {}   lines/sec = {}",
            secs, a.lines_read, rate
        );
    }
    let count = format!("{}: Number of errors = {}\n", p, a.errors);
    let _ = write!(out, "{}", count);
    let _ = out.flush();

    a.write_outputs(&obj_name, &lst_name, &exp_name, &sym_name, &count);

    // 1.8: status 1 whenever the error count is non-zero -- and the object and
    // listing files are still written.
    std::process::exit(if a.errors > 0 { EXIT_ERRORS } else { EXIT_OK });
}

//! tabasm -- a clean-room reimplementation of the TASM cross-assembler.
//!
//! Built from the specification alone. See AGENTS.md: no part of this descends from the
//! original implementation's source or binaries.

#![allow(dead_code)] // constants and helpers land ahead of their first use

mod cli;
mod expr;
mod table;
mod limits;

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

    // M0: the skeleton stops here. Assembly proper arrives with M1.
    let _ = out.flush();
    std::process::exit(EXIT_OK);
}

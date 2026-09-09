//! tabasm -- a clean-room reimplementation of the TASM cross-assembler.
//!
//! Written from a behavioural specification alone: no part of this descends
//! from the original implementation's source or binaries.

#![allow(dead_code)] // the exit codes and message catalogue are kept complete

mod asm;
mod cli;
mod errlog;
mod expr;
mod image;
mod limits;
mod listing;
mod macros;
mod matcher;
mod object;
mod rules;
mod symbols;
mod table;
mod table2;

use cli::Options;
use limits::*;
use std::io::Write;

/// The prefix on the assembler's own messages -- the ones that are not
/// per-line diagnostics: the progress lines, the error count, and the fatal
/// reports.
///
/// It is `--message-prefix`, defaulting to the program's own name. TASM wrote
/// `tasm:` here, so reproducing its output byte-for-byte is a matter of passing
/// `--message-prefix tasm` rather than of a hidden mode.
///
/// This covers stderr as well as stdout: the TASMERRFORMAT rejection warning is
/// prefixed too, and is compared.
///
/// Every such message must route through here rather than embedding a literal.
pub fn prog(o: &Options) -> &str {
    &o.message_prefix
}

/// The two-line identification banner is the one thing excluded when output is
/// compared against the original's, precisely so that a clean-room
/// reimplementation prints its own identification rather than inheriting
/// someone else's. Keep it to two lines: the comparison skips exactly that many.
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
    let _ = writeln!(
        out,
        " Clean-room reimplementation from the TASM behavioural specification."
    );
}

/// `--help`. Not `-h`: that is the hex dump, and was before this program
/// existed. The full description is in tabasm(1); this is the reminder.
fn usage(out: &mut impl Write) {
    let _ = write!(
        out,
        "\nusage: tabasm --cpu <name> [options] source [object [listing [export [symbol]]]]

  --cpu <name>              instruction table to use, required
  --compatibility           original expression order; .UNDEF does nothing
  --bug-compatibility       restore two defects the original had
  --message-prefix <name>   name on the assembler's own messages
  --page-title <text>       heading for a paged listing
  --help                    this text

  -a[xx]  strict-check mask, hex; bare -a sets all bits    (default 06)
  -b      binary object output and block mode              (= -g3 -c)
  -c      block mode: a counter change does not split records
  -d<name>  define <name>, as #define would
  -e      list macro lines expanded rather than as written
  -f<xx>  fill byte for locations the source never writes
  -g<n>   object format: 0 Intel HEX, 1 MOS Technology, 2 Motorola S-record,
          3 raw binary, 4 Intel HEX with word addresses    (default 0)
  -h      append a hex dump to the listing
  -i      fold case in symbol names
  -l      append the label table; -ll long form, -la all labels
  -m      MOS Technology object format                     (= -g1)
  -o<xx>  bytes per object record, hex                     (default 18)
  -p<n>   page length in lines, decimal
  -q      suppress the listing
  -s      write the symbol file
  -x<d>   instruction class mask, hex digit; bare -x all   (default 1)
  -y      print elapsed time and line count
  -z      write a trace to standard error

Only the source is required; the other names default to the source base name
with .obj, .lst, .exp and .sym.

Tables are looked for in $TASMTABS, the working directory, beside the
executable, and the usual share directories. See tabasm(1).
"
    );
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
    if o.help {
        usage(&mut out);
        let _ = out.flush();
        std::process::exit(EXIT_OK);
    }
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
    let look = cli::Lookup::from_env();
    let paths = o.table_paths(&look);
    let table = match paths.first().cloned() {
        Some(path) => match table::Table::load_first(&paths, o.table.as_deref().unwrap_or("")) {
            Ok(t) => t,
            Err(table::LoadError::Open(p)) => {
                // With more than one place to look, "cannot open" is not much
                // help on its own: say where it looked.
                let _ = writeln!(out, "{}: cannot open table file {}", prog(&o), p);
                for c in &paths {
                    let _ = writeln!(out, "{}:   tried {}", prog(&o), c);
                }
                let _ = out.flush();
                std::process::exit(EXIT_FILE);
            }
            Err(table::LoadError::TooManyRows) => {
                let _ = writeln!(out, "{}: Max number of instructions exceeded", prog(&o));
                let _ = out.flush();
                std::process::exit(EXIT_FATAL);
            }
            Err(table::LoadError::Syntax(line, what)) => {
                // A v2 table that does not parse. 1.5 makes a table that cannot
                // be opened fatal with exit 3; one that cannot be understood is
                // the same class of failure, and now says where.
                let _ = writeln!(out, "{}: {} line {}: {}", prog(&o), path, line, what);
                let _ = out.flush();
                std::process::exit(EXIT_FILE);
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

    if o.debug {
        eprintln!(
            "[trace] tabasm {} ({} rows, {} regsets) selector={} wordaddrs={} msfirst={}",
            env!("CARGO_PKG_VERSION"),
            table.rows.len(),
            table.regsets.len(),
            table.selector,
            table.wordaddrs,
            table.msfirst
        );
    }

    let source = o.source().unwrap().to_string();
    let (base, base_warn) = o.base_name();
    if let Some(w) = base_warn {
        let _ = writeln!(out, "{}: {}", prog(&o), w);
    }
    let obj_name = o.out_name(1, ".obj", &base);
    let lst_name = o.out_name(2, ".lst", &base);
    let exp_name = o.out_name(3, ".exp", &base);
    let sym_name = o.out_name(4, ".sym", &base);

    let p = o.message_prefix.clone();
    let mut a = asm::Asm::new(o, table, fmt);
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
        let rate = if secs > 0.0 {
            (a.lines_read as f64 / secs) as u64
        } else {
            0
        };
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

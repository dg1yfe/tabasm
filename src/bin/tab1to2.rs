//! `tab1to2` — convert a legacy instruction table to the current format.
//!
//!     tab1to2 <in.tab> <out.tab2> [banner]
//!
//! The legacy format is positional and carries no structure for commentary, so
//! everything that is not a directive or an instruction row is dropped — which
//! is also what strips any authorship or revision headers a table may carry.
//! Supplying `banner` replaces the table's own.

use std::process::ExitCode;

#[path = "../cli.rs"]
mod cli;
#[path = "../errlog.rs"]
mod errlog;
#[path = "../expr.rs"]
mod expr;
#[path = "../limits.rs"]
mod limits;
#[path = "../table.rs"]
mod table;
#[path = "../table2.rs"]
mod table2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: tab1to2 <in.tab> <out.tab2> [banner]");
        return ExitCode::from(2);
    }
    let (input, output) = (&args[0], &args[1]);
    // The selector only matters for a legacy table's auxiliary-register width,
    // which the converted table then states outright.
    let selector = std::path::Path::new(input)
        .file_stem()
        .map(|s| s.to_string_lossy().trim_start_matches("tasm").to_string())
        .unwrap_or_default();

    let t = match table::Table::load(input, &selector) {
        Ok(t) => t,
        Err(table::LoadError::Open(p)) => {
            eprintln!("tab1to2: cannot open {}", p);
            return ExitCode::from(3);
        }
        Err(table::LoadError::Syntax(line, what)) => {
            eprintln!("tab1to2: {} line {}: {}", input, line, what);
            return ExitCode::from(1);
        }
        Err(_) => {
            eprintln!("tab1to2: {} exceeds a table limit", input);
            return ExitCode::from(4);
        }
    };

    let text = table2::render(&t, args.get(2).map(|s| s.as_str()));
    if let Err(e) = std::fs::write(output, &text) {
        eprintln!("tab1to2: cannot write {}: {}", output, e);
        return ExitCode::from(3);
    }
    eprintln!(
        "tab1to2: {} -> {} ({} rows, {} register-set entries)",
        input,
        output,
        t.rows.len(),
        t.regsets.len()
    );
    ExitCode::SUCCESS
}

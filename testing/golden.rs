//! Regression: every case must reproduce its recorded output byte for byte.
//!
//! This suite answers one question — "does the assembler still do what it did
//! yesterday?" — and nothing else. It is deliberately dumb: it compares bytes.
//! What makes those bytes trustworthy in the first place is
//! `testing/conformance.rs`, which checks them against the tables and against
//! arithmetic rather than against memory.
//!
//! To re-record after an intended change:
//!
//!     TABASM_FREEZE=1 cargo test --test golden
//!
//! Never do that to make a failure go away. A golden changes when the change is
//! intended and explained, and the diff is the review.

mod common;
use common::*;

struct Case {
    id: &'static str,
    args: &'static [&'static str],
    src: &'static str,
    env: &'static [(&'static str, &'static str)],
}

const fn c(id: &'static str, args: &'static [&'static str], src: &'static str) -> Case {
    Case { id, args, src, env: &[] }
}

/// The eleven targets, each with the object formats. Breadth across processors
/// and encodings; `conformance.rs` supplies the depth.
const TARGETS: &[(&str, &str)] = &[
    ("8048", "--cpu=8048"),
    ("6502", "--cpu=6502"),
    ("6800", "--cpu=6800"),
    ("6805", "--cpu=6805"),
    ("8051", "--cpu=8051"),
    ("8085", "--cpu=8085"),
    ("8096", "--cpu=8096"),
    ("z80", "--cpu=z80"),
    ("tms7000", "--cpu=tms7000"),
    ("tms32010", "--cpu=tms32010"),
    ("tms320c25", "--cpu=tms320c25"),
];

const FORMATS: &[(&str, &str)] = &[
    ("", ""),
    ("-g1", "-g1"),
    ("-g2", "-g2"),
    ("-g3", "-g3"),
    ("-g4", "-g4"),
    ("-c", "-c"),
];

/// Hand-written cases, each pinning a documented behaviour.
const CASES: &[Case] = &[
    // Listing and object options, on one target to keep the corpus small.
    c("opt-labels", &["--cpu=8051", "-l"], "testing/smoke/8051.asm"),
    c("opt-labels-long", &["--cpu=8051", "-ll"], "testing/smoke/8051.asm"),
    c("opt-labels-all", &["--cpu=8051", "-la"], "testing/smoke/8051.asm"),
    c("opt-hex", &["--cpu=8051", "-h"], "testing/smoke/8051.asm"),
    c("opt-paged", &["--cpu=8051", "-p20"], "testing/smoke/8051.asm"),
    c("opt-quiet", &["--cpu=8051", "-q"], "testing/smoke/8051.asm"),
    c("opt-expand", &["--cpu=8051", "-e"], "testing/smoke/8051.asm"),
    c("opt-ignorecase", &["--cpu=8051", "-i"], "testing/smoke/8051.asm"),
    c("opt-fill", &["--cpu=8051", "-c", "-fAA"], "testing/smoke/8051.asm"),
    c("opt-rec8", &["--cpu=8051", "-o8"], "testing/smoke/8051.asm"),
    c("opt-rec32", &["--cpu=8051", "-o32"], "testing/smoke/8051.asm"),
    c("opt-title", &["--cpu=8051", "-p20", "--page-title=Some Title"], "testing/smoke/8051.asm"),
    c("opt-prefix", &["--cpu=8051", "--message-prefix=xyz"], "testing/smoke/8051.asm"),
    // Diagnostics.
    c("err-undef", &["--cpu=8051"], "testing/cases/err-undef.asm"),
    c("err-badinst", &["--cpu=8051"], "testing/cases/err-badinst.asm"),
    c("err-baddir", &["--cpu=8051"], "testing/cases/err-baddir.asm"),
    c("err-range", &["--cpu=8051"], "testing/cases/err-range.asm"),
    c("end-bigaddr", &["--cpu=8051", "-g2"], "testing/cases/end-bigaddr.asm"),
    c("arg-checks", &["--cpu=8051", "-a"], "testing/cases/arg-checks.asm"),
    // Expressions, macros, symbols.
    c("expressions", &["--cpu=8051"], "testing/cases/expressions.asm"),
    c("expressions-compat", &["--cpu=8051", "--compatibility"], "testing/cases/expressions-compat.asm"),
    c("undef", &["--cpu=8051"], "testing/cases/undef.asm"),
    c("undef-compat", &["--cpu=8051", "--compatibility"], "testing/cases/undef.asm"),
    c("macro-params", &["--cpu=8051"], "testing/cases/macro-params.asm"),
    c("macro-deep", &["--cpu=8051"], "testing/cases/macro-deep.asm"),
    c("comment-char", &["--cpu=8051"], "testing/cases/comment-char.asm"),
    c("symfile-plain", &["--cpu=8051", "-s"], "testing/cases/symfile-plain.asm"),
    c("symfile-segments", &["--cpu=8051"], "testing/cases/symfile-segments.asm"),
    c("export-file", &["--cpu=8051"], "testing/cases/export-file.asm"),
    c("negative-values", &["--cpu=8051", "-l"], "testing/cases/negative-values.asm"),
    c("negative-values-ll", &["--cpu=8051", "-ll"], "testing/cases/negative-values.asm"),
    c("label-sort", &["--cpu=8051", "-l"], "testing/cases/label-sort.asm"),
    c("addinstr-sameinst", &["--cpu=8051"], "testing/cases/addinstr-sameinst.asm"),
    c("crlf-source", &["--cpu=8051"], "testing/cases/crlf-source.asm"),
    c("rules-unshipped", &["--cpu=8051"], "testing/cases/rules-unshipped.asm"),
    // Target-specific behaviours.
    c("zp-phase", &["--cpu=6502"], "testing/cases/zp-phase.asm"),
    c("z80-im-alias", &["--cpu=z80", "-x"], "testing/cases/z80-im-alias.asm"),
    c("tms-arp-c25", &["--cpu=tms320c25"], "testing/cases/tms-arp-width.asm"),
    c("tms-arp-32010", &["--cpu=tms32010"], "testing/cases/tms-arp-width.asm"),
    c("tms-wordaddr", &["--cpu=tms32010"], "testing/cases/tms-wordaddr.asm"),
    // The two defects the default corrects, pinned in both modes.
    c("bug-sort", &["--cpu=8051", "-l", "--bug-compatibility"], "testing/cases/label-sort.asm"),
    c("bug-g4", &["--cpu=8051", "-g4", "--bug-compatibility"], "testing/smoke/8051.asm"),
];

/// TASMERRFORMAT, which is a printf format applied to four arguments and must be
/// validated rather than trusted.
const ERRFMT: &[(&str, &str)] = &[
    ("errfmt-default", ""),
    ("errfmt-custom", "%s(%d): %s %s"),
    ("errfmt-short", "%s"),
    ("errfmt-flags", "%-20s %04d %s%s"),
    ("errfmt-mismatch", "%s %s %s %s"),
    ("errfmt-percent-n", "%s %n"),
    ("errfmt-bigwidth", "%99999999s"),
];

const ARTEFACTS: [&str; 7] = ["obj", "lst", "exp", "sym", "out", "err", "exit"];

fn golden_dir() -> std::path::PathBuf {
    root().join("testing/golden")
}

/// Compare or record one case.
fn check(id: &str, run: &Run, failures: &mut Vec<String>) {
    let freeze = std::env::var("TABASM_FREEZE").is_ok();
    let dir = golden_dir();
    if freeze {
        std::fs::create_dir_all(&dir).expect("golden directory");
    }
    for ext in ARTEFACTS {
        let path = dir.join(format!("{}.{}", id, ext));
        let ours: Option<Vec<u8>> = match ext {
            "obj" => run.obj.clone(),
            "lst" => run.lst.clone(),
            "exp" => run.exp.clone(),
            "sym" => run.sym.clone(),
            "out" => Some(run.out.clone()),
            "err" => Some(run.err.clone()),
            _ => Some(format!("{}\n", run.code.unwrap_or(-1)).into_bytes()),
        };
        if freeze {
            match &ours {
                Some(b) => std::fs::write(&path, b).expect("write golden"),
                None => {
                    let _ = std::fs::remove_file(&path);
                }
            }
            continue;
        }
        let theirs = std::fs::read(&path).ok();
        if ours == theirs {
            continue;
        }
        // A missing artefact on either side is as much a failure as a differing
        // one, and worth saying which.
        failures.push(match (&ours, &theirs) {
            (Some(_), None) => format!("{}.{}: produced, but no golden recorded", id, ext),
            (None, Some(_)) => format!("{}.{}: golden recorded, but not produced", id, ext),
            _ => {
                let (a, b) = (ours.unwrap(), theirs.unwrap());
                format!(
                    "{}.{}: differs ({} bytes vs {}); first at offset {}",
                    id,
                    ext,
                    a.len(),
                    b.len(),
                    a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()))
                )
            }
        });
    }
}

#[test]
fn every_case_reproduces_its_recorded_output() {
    let mut failures = Vec::new();
    let mut n = 0;

    for (name, cpu) in TARGETS {
        for (tag, flag) in FORMATS {
            let id = if tag.is_empty() {
                format!("{}", name)
            } else {
                format!("{}{}", name, tag)
            };
            let scratch = Scratch::new("golden");
            let mut args = vec![*cpu];
            if !flag.is_empty() {
                args.push(flag);
            }
            // Every target's extended set, so those rows are reachable.
            args.push("-x");
            let src = format!("testing/smoke/{}.asm", name);
            let run = assemble(&args, &src, &scratch, &[]);
            check(&id, &run, &mut failures);
            n += 1;
        }
    }

    for case in CASES {
        let scratch = Scratch::new("golden");
        let run = assemble(case.args, case.src, &scratch, case.env);
        check(case.id, &run, &mut failures);
        n += 1;
    }

    for (id, fmt) in ERRFMT {
        let scratch = Scratch::new("golden");
        let env: Vec<(&str, &str)> =
            if fmt.is_empty() { vec![] } else { vec![("TASMERRFORMAT", *fmt)] };
        let run = assemble(&["--cpu=8051"], "testing/cases/err-undef.asm", &scratch, &env);
        check(id, &run, &mut failures);
        n += 1;
    }

    if std::env::var("TABASM_FREEZE").is_ok() {
        eprintln!("recorded {} cases into testing/golden/", n);
        return;
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from their recorded output:\n  {}",
        failures.len(),
        n,
        failures.join("\n  ")
    );
    eprintln!("{} cases reproduced byte for byte", n);
}

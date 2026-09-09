//! Robustness: hostile input must not crash, hang, or corrupt memory.
//!
//! These assert *survival*, never particular bytes. A non-zero exit is fine —
//! reporting an error is the right answer to most of this. What is not fine is a
//! signal, a hang, or a diagnostic that suggests memory was scribbled on.
//!
//! Run under a build that traps arithmetic to make them mean more:
//!
//!     RUSTFLAGS='-C overflow-checks=on -C debug-assertions=on' cargo test --test robustness
//!
//! The threat model is that someone hands you a source file, or a table, or sets
//! an environment variable, and you run the assembler on it.

mod common;
use common::*;
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(30);

/// Assemble `body` for the 8051 and require the run to survive.
fn survives(name: &str, body: &str) {
    let scratch = Scratch::new("hostile");
    let path = scratch.path("t.asm");
    std::fs::write(&path, format!("        .org 0\n{}\n        .end\n", body)).unwrap();
    let run = assemble_bounded(
        &["--cpu=8051"],
        path.to_str().unwrap(),
        &scratch,
        &[],
        LIMIT,
    );
    let run = run.unwrap_or_else(|| panic!("{}: hung", name));
    assert!(!run.crashed(), "{}: killed by signal {:?}", name, run.signal);
}

/// As `survives`, but the hostile input is the table rather than the source.
fn survives_table(name: &str, table: &str, body: &str) {
    let scratch = Scratch::new("hostile-tab");
    std::fs::write(scratch.path("hostile.tab2"), table).unwrap();
    let path = scratch.path("t.asm");
    std::fs::write(&path, format!("        .org 0\n{}\n        .end\n", body)).unwrap();
    let mut cmd = std::process::Command::new(TABASM);
    cmd.current_dir(&scratch.0)
        .env("TASMTABS", &scratch.0)
        .args(["--cpu=hostile", "t.asm", "a.obj", "a.lst"]);
    let out = cmd.output().expect("run");
    // Where signals exist, a signal is the failure. Elsewhere the assertion is
    // that it ran to completion at all.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            out.status.signal().is_none(),
            "{}: killed by signal {:?}",
            name,
            out.status.signal()
        );
    }
    #[cfg(not(unix))]
    assert!(out.status.code().is_some(), "{}: did not exit normally", name);
}

#[test]
fn hostile_expressions() {
    survives("divide by zero", "        .byte 1/0");
    survives("modulo by zero", "        .byte 1%0");
    survives("INT_MIN / -1", "        .byte (1<<31)/-1");
    survives("shift count 32", "        .byte 1<<32");
    survives("negative shift", "        .byte 1<<-1");
    survives("right shift 64", "        .byte 1>>64");
    survives("overflow", "        .byte 2000000000+2000000000\n        .byte 100000*100000");
    survives("500-digit literal", &format!("        .byte {}", "9".repeat(500)));
    survives("400-character symbol", &format!("        .byte {}", "A".repeat(400)));
    survives("deep parens", &format!("        .byte {}1{}", "(".repeat(200), ")".repeat(200)));
    survives("unary chain", &format!("        .byte {}1", "-".repeat(300)));
}

#[test]
fn hostile_counters_and_addresses() {
    survives("org at the top", "        .org 0FFFFh\n        .byte 1,2,3,4");
    survives("org beyond the image", "        .org 20000h\n        .byte 1");
    survives("org huge", "        .org 7FFFFFFFh\n        .byte 1");
    survives("org negative", "        .org -1\n        .byte 1");
    survives("fill negative", "        .fill -1");
    survives("fill past the end", "        .org 0FFF0h\n        .fill 200");
    survives("chk at zero", "        .chk 0");
    survives("block huge", "        .block 7FFFFFFFh");
}

#[test]
fn hostile_conditionals_and_macros() {
    survives("unmatched endif", "        .endif\n        .endif");
    survives("unmatched else", "        .else");
    let deep: String = (0..40).map(|_| "#ifdef FOO\n").collect::<String>()
        + "        nop\n"
        + &(0..40).map(|_| "#endif\n").collect::<String>();
    survives("40 nested ifdef", &deep);
    survives("bare define", "#define");
    survives("defcont with no define", "#defcont x");
    survives("cyclic macro", "#define A A\n        A");
    survives("mutually cyclic", "#define A B\n#define B A\n        A");
    survives("cyclic with parameters", "#define F(a) F(a)\n        F(9)");
    survives("parameter with no argument", "#define M ?0\n        .byte M");
    let many: String = (0..1100).map(|i| format!("#define M{} {}\n", i, i)).collect();
    survives("1100 macros", &many);
    let long: String = format!("#define BIG {}\n", "A".repeat(400))
        + &(0..8).map(|_| format!("#defcont {}\n", "B".repeat(400))).collect::<String>()
        + "        .byte BIG\n";
    survives("macro body past the line buffer", &long);
}

#[test]
fn hostile_strings_and_lines() {
    survives("high-bit bytes", r#"        .byte "\200\201\202""#);
    survives("trailing backslash", r#"        .text "abc\"#);
    survives("truncated octal", r#"        .text "abc\1"#);
    survives("unterminated string", r#"        .text "abc"#);
    survives("500-character line", &format!("        nop ;{}", "x".repeat(490)));
    survives("many operands", &format!("        .byte {}", "1,".repeat(400) + "1"));
}

#[test]
fn hostile_addinstr() {
    // .ADDINSTR adds a table row from the source, so table-parser paths are
    // reachable from an ordinary input file.
    survives("addinstr mnemonic only", "        .addinstr FOO");
    survives("addinstr no operand", "        .addinstr");
    survives("addinstr long mnemonic", &format!("        .addinstr {}", "A".repeat(600)));
    survives("addinstr 200-digit opcode", &format!("        .addinstr FOO * {} 3 NO 1", "A".repeat(200)));
    survives("addinstr byte count under opcode", "        .addinstr FOO *,* 12 FF ZW 1\n        FOO 1,2");
}

#[test]
fn hostile_tables() {
    let head = "%format 2\n%columns mnemonic operands opcode op-bytes arg-bytes rule\n";
    survives_table("empty table", "", "        nop");
    survives_table("no format directive", "NOP - 00 1 0 plain\n", "        nop");
    survives_table("truncated row", &format!("{}NOP\n", head), "        nop");
    survives_table(
        "200-digit opcode",
        &format!("{}NOP - {} 1 0 plain\n", head, "A".repeat(200)),
        "        nop",
    );
    survives_table(
        "absurd byte counts",
        &format!("{}NOP - 00 99 99 plain\n", head),
        "        nop",
    );
    survives_table(
        "200 placeholders",
        &format!("{}FOO {} 12 1 2 plain\n", head, "<expr>,".repeat(200) + "<expr>"),
        "        foo 1",
    );
    survives_table(
        "format specifiers in a pattern",
        &format!("{}NOP %s%s%n 00 1 0 plain\n", head),
        "        nop",
    );
    survives_table(
        "unknown rule",
        &format!("{}NOP - 00 1 0 no-such-rule\n", head),
        "        nop",
    );
    survives_table("legacy table, mnemonic only", "\"X\"\nNOP\n", "        nop");
    survives_table("legacy table, no banner quote", "no quote\nNOP \"\" 00 1 NOP 1\n", "        nop");
}

#[test]
fn hostile_command_line_and_environment() {
    let scratch = Scratch::new("cli");
    let path = scratch.path("t.asm");
    std::fs::write(&path, "        .org 0\n        nop\n        .end\n").unwrap();
    let src = path.to_str().unwrap();

    for (name, args) in [
        ("eight file names", vec!["--cpu=8051", "a", "b", "c", "d", "e", "f", "g"]),
        ("-d with 9000 characters", vec!["--cpu=8051"]),
        ("-o beyond a record", vec!["--cpu=8051", "-offff"]),
        ("-p zero", vec!["--cpu=8051", "-p0"]),
        ("-x absurd", vec!["--cpu=8051", "-xFFFFFFFF"]),
        ("-f absurd", vec!["--cpu=8051", "-fFFFFFFFF"]),
        ("no such cpu", vec!["--cpu=..%2f..%2fetc"]),
        ("empty cpu", vec!["--cpu="]),
    ] {
        let mut a = args.clone();
        let big = format!("-d{}", "A".repeat(9000));
        if name == "-d with 9000 characters" {
            a.push(&big);
        }
        let run = assemble_bounded(&a, src, &scratch, &[], LIMIT)
            .unwrap_or_else(|| panic!("{}: hung", name));
        assert!(!run.crashed(), "{}: killed by signal {:?}", name, run.signal);
    }

    // TASMERRFORMAT is a printf format applied to four arguments. An
    // unvalidated one was a format-string vulnerability in the original.
    for fmt in ["%s %s %s %s", "%s %n", "%99999999s", "%d %d %d %d", "%*s", "%", "%1$s"] {
        let run = assemble_bounded(
            &["--cpu=8051"],
            "testing/cases/err-undef.asm",
            &scratch,
            &[("TASMERRFORMAT", fmt)],
            LIMIT,
        )
        .unwrap_or_else(|| panic!("TASMERRFORMAT {:?}: hung", fmt));
        assert!(!run.crashed(), "TASMERRFORMAT {:?}: killed by signal {:?}", fmt, run.signal);
    }

    // TASMOPTS is appended to the command line.
    let opts = "-q ".repeat(300);
    let run = assemble_bounded(&["--cpu=8051"], src, &scratch, &[("TASMOPTS", &opts)], LIMIT)
        .unwrap_or_else(|| panic!("TASMOPTS: hung"));
    assert!(!run.crashed(), "TASMOPTS: killed by signal {:?}", run.signal);
}

/// A full 64 KB image must not silently emit an empty object: the span
/// computation has to survive the top of the address space.
#[test]
fn a_full_image_still_emits_records() {
    let scratch = Scratch::new("full");
    let path = scratch.path("full.asm");
    std::fs::write(&path, "        .org 0\n        .fill 0FFFFh\n        .byte 42h\n        .end\n")
        .unwrap();
    let run = assemble_bounded(
        &["--cpu=8051", "-c"],
        path.to_str().unwrap(),
        &scratch,
        &[],
        Duration::from_secs(60),
    )
    .expect("hung on a full image");
    assert!(!run.crashed(), "killed by signal {:?}", run.signal);
    let obj = run.text("obj");
    assert!(obj.lines().filter(|l| l.starts_with(':')).count() > 2, "object is near-empty");
    assert!(obj.contains("42"), "the last byte is missing from the object");
}

//! Shared plumbing for the test suites.
//!
//! Every suite drives the built binary through `Command` — argv, environment,
//! files, exit status — so the tests stay black-box even though they live in the
//! same tree. `CARGO_BIN_EXE_tabasm` is the binary cargo has just built, which
//! removes any chance of testing a stale copy.

#![allow(dead_code)] // each suite uses a different subset

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const TABASM: &str = env!("CARGO_BIN_EXE_tabasm");

/// The repository root, so a suite can reach `tables/` and `testing/` without
/// depending on the working directory cargo chose.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A scratch directory that removes itself. No external crate, and no reliance
/// on `mktemp`.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tabasm-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Scratch(dir)
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copy a source into the scratch directory and return the bare file name to
/// put on the command line, so nothing in the output depends on where this
/// checkout lives.
fn local_copy(source: &str, scratch: &Scratch) -> String {
    let from = if Path::new(source).is_absolute() {
        PathBuf::from(source)
    } else {
        root().join(source)
    };
    let name = from
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input.asm".to_string());
    let to = scratch.path(&name);
    if from != to {
        std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {}: {}", from.display(), e));
    }
    // A case that includes another file needs that file beside it. Only
    // siblings are followed, and only one level: enough for a case that pins
    // how an include is listed, and no more.
    if let Ok(text) = std::fs::read_to_string(&from) {
        for line in text.lines() {
            let t = line.trim_start();
            if t.len() < 9 || !t[..9].eq_ignore_ascii_case("#include ") {
                continue;
            }
            let inc = t[9..].trim().trim_matches('"');
            if inc.is_empty() || inc.contains('/') {
                continue;
            }
            if let Some(dir) = from.parent() {
                let _ = std::fs::copy(dir.join(inc), scratch.path(inc));
            }
        }
    }
    name
}

/// Everything one assembly produced.
pub struct Run {
    pub obj: Option<Vec<u8>>,
    pub lst: Option<Vec<u8>>,
    pub exp: Option<Vec<u8>>,
    pub sym: Option<Vec<u8>>,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
    pub code: Option<i32>,
    /// The signal that killed it, if any. Checking this directly is stricter
    /// than searching stderr for a crash message, and it is portable.
    pub signal: Option<i32>,
}

impl Run {
    pub fn text(&self, which: &str) -> String {
        let bytes = match which {
            "obj" => self.obj.clone().unwrap_or_default(),
            "lst" => self.lst.clone().unwrap_or_default(),
            "exp" => self.exp.clone().unwrap_or_default(),
            "sym" => self.sym.clone().unwrap_or_default(),
            "out" => self.out.clone(),
            _ => self.err.clone(),
        };
        String::from_utf8_lossy(&bytes).into_owned()
    }
    pub fn crashed(&self) -> bool {
        self.signal.is_some()
    }
}

/// Assemble `source` with `args`, writing the five positional outputs into
/// `scratch`. `source` is relative to the repository root unless absolute.
///
/// The source is copied into the scratch directory and named on the command
/// line without a path. Diagnostics and the paged-listing heading both quote the
/// name they were given, so passing an absolute one would bake this checkout's
/// location into every recorded artefact.
pub fn assemble(args: &[&str], source: &str, scratch: &Scratch, env: &[(&str, &str)]) -> Run {
    let src = local_copy(source, scratch);
    let names = ["a.obj", "a.lst", "a.exp", "a.sym"];
    let mut cmd = Command::new(TABASM);
    cmd.current_dir(&scratch.0);
    // Tables are found through the environment, as a user would.
    cmd.env("TASMTABS", root().join("tables"));
    cmd.env_remove("TASMOPTS");
    cmd.env_remove("TASMERRFORMAT");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.args(args).arg(&src);
    for n in names {
        cmd.arg(n);
    }
    let out = cmd.output().expect("failed to run tabasm");
    let read = |n: &str| std::fs::read(scratch.path(n)).ok();
    Run {
        obj: read("a.obj"),
        lst: read("a.lst"),
        exp: read("a.exp"),
        sym: read("a.sym"),
        out: out.stdout,
        err: out.stderr,
        code: out.status.code(),
        signal: signal_of(&out.status),
    }
}

#[cfg(unix)]
fn signal_of(s: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    s.signal()
}

#[cfg(not(unix))]
fn signal_of(_: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// Run with a wall-clock limit, so a hang is a failure rather than a hung suite.
/// Polling `try_wait` keeps this dependency-free.
pub fn assemble_bounded(
    args: &[&str],
    source: &str,
    scratch: &Scratch,
    env: &[(&str, &str)],
    limit: std::time::Duration,
) -> Option<Run> {
    let src = local_copy(source, scratch);
    let mut cmd = Command::new(TABASM);
    cmd.current_dir(&scratch.0)
        .env("TASMTABS", root().join("tables"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.args(args).arg(&src).arg("a.obj").arg("a.lst");
    let mut child = cmd.spawn().expect("failed to spawn tabasm");
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().expect("wait") {
            Some(status) => {
                let out = child.wait_with_output().expect("output");
                return Some(Run {
                    obj: std::fs::read(scratch.path("a.obj")).ok(),
                    lst: std::fs::read(scratch.path("a.lst")).ok(),
                    exp: None,
                    sym: None,
                    out: out.stdout,
                    err: out.stderr,
                    code: status.code(),
                    signal: signal_of(&status),
                });
            }
            None if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None; // a hang
            }
            None => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
}

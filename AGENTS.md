# Working on tabasm

## What it is

A table-driven, two-pass, absolute cross-assembler for 8- and early-16-bit
microprocessors. It is an independent reimplementation of TASM, output-compatible
with it, correcting two of its defects by default and adding a self-describing
table format.

Three properties shape everything else:

1. **The instruction set is data, not code.** Processors are described by
   `tables/*.tab2`, read at run time. Supporting a new one normally means writing
   a table; only a genuinely new *encoding rule* — a way of folding operand values
   into instruction bytes — needs a change to the program.
2. **Output is absolute.** No linker, no relocation. One flat 64 KB image, written
   out in one of five object formats.
3. **One source file at a time**, composed with `#include`.

## Language and build

Rust, no dependencies, no `unsafe`. `make` builds, `make install` installs to
`$PREFIX` with the tables where the assembler looks for them, and `DESTDIR`
stages that under another root. `cargo build --release` does the build on its
own. `make -C src` is a separate convenience: a binary at a fixed path,
`SANITIZE=1` for a build that traps arithmetic overflow, and `checksec` to
report what the binary is. `cargo test` runs everything.

## Style

- **Fixed widths, explicit arithmetic.** Values are 32-bit by specification, not
  by whatever the host's word happens to be. Use `wrapping_*` where wrapping is
  the defined behaviour and guard where it is not — division, modulo, shift counts
  and `INT_MIN / -1` all have defined answers here, and none of them is a panic.
- **No panics on any path reachable from input.** A source file and a table are
  both untrusted: a table drives the encoder, and `.ADDINSTR` lets a source file
  add table rows, so the table parser is reachable from ordinary input.
- **Comments say why.** What the code does is visible; why it does something
  surprising is not. Where behaviour looks wrong, say what pins it.
- **Match the surrounding code** rather than introducing a second idiom.

Comments carry numbers like `7.20` or `4.5`. Those cite sections of the
behavioural specification tabasm was written from, which is not published: it
describes the original in detail, and this project reimplements that behaviour
rather than redistributing the description of it. The comments are written to
stand without it, so treat a number as an attribution and read the sentence
around it. New comments do not need one.

## Testing

`cargo test` is the only entry point. There is nothing to install.

| Suite | Question it answers |
|---|---|
| unit tests, in `src/*.rs` | do the pieces behave? |
| `testing/conformance.rs` | is the output *right*? |
| `testing/golden.rs` | is the output *unchanged*? |
| `testing/robustness.rs` | does hostile input crash or hang it? |

Two conventions matter:

**`conformance.rs` deliberately shares no code with `src/`.** It carries its own
readers for the table format, object records and listing lines. A suite that
parsed tables with the assembler's own parser would agree with it about a
misparse and prove nothing. Keep that duplication.

**Never re-record a golden to make a failure go away.** `TABASM_FREEZE=1 cargo
test --test golden` exists for intended changes, and the resulting diff is the
review. Run `cargo test --test conformance` first: freezing output that nothing
has independently checked casts a bug in stone.

For a build that traps arithmetic rather than wrapping it silently:

```
RUSTFLAGS='-C overflow-checks=on -C debug-assertions=on' cargo test
```

## What CI checks

`.github/workflows/ci.yml`, on every push and pull request:

- `cargo test --release` on **Linux, macOS and Windows**. The golden suite
  compares against output recorded on one machine, so running it on three is a
  portability check as much as a build check — the assembler must emit identical
  bytes everywhere.
- the overflow-checked build, which traps what release wraps;
- `cargo fmt --check` and `cargo clippy -- -D warnings`, both of which are clean;
- the minimum supported Rust version, read from `Cargo.toml` so the claim cannot
  drift;
- that the tree stands alone: build it and assemble every target directly,
  without going through `cargo test`.

Before pushing, the same gates locally:

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --release
```

# tabasm

A table-driven, two-pass, absolute cross-assembler for 8- and early-16-bit
microprocessors. Twelve targets are supported out of the box, and adding another
usually means writing a table rather than changing the program.

tabasm is an independent reimplementation of the **Telemark Assembler (TASM)**,
a table-driven cross-assembler first released in 1985. It reproduces TASM's
output byte for byte where asked to, corrects two of its defects by default, and
adds a table format that says what it means.

## Installing

Binaries for Linux, macOS and Windows are attached to each
[release](https://github.com/dg1yfe/tabasm/releases), with a `SHA256SUMS` to
check them against. Unpack and run, or install with the commands under
*Where the tables are found* below.

On macOS, via Homebrew:

```
brew trust dg1yfe/tap
brew install dg1yfe/tap/tabasm
```

The first line is not optional: Homebrew 6 refuses to load formulae from a
third-party tap until it is trusted, and the failure reports *invalid syntax in
tap*, which points at the formula rather than at the trust setting.

Homebrew is macOS only here. On Linux, take the release tarball — the builds
are statically linked and run on any distribution.

The release archives carry the assembler alone. `tab1to2`, which rewrites a
legacy table in the current format, is built from this repository; the
assembler reads legacy tables directly, so converting one is a choice rather
than a requirement.

## Building and installing from source

```
make                           # build
sudo make install              # to /usr/local
sudo make uninstall
```

`make install` puts the assembler in `$PREFIX/bin` and the tables in
`$PREFIX/share/tabasm/tables`, which is where it looks for them, so nothing
needs setting afterwards. The man page and documentation go alongside.
`PREFIX` defaults to `/usr/local`; `DESTDIR` stages the whole install under
another root without touching the real filesystem:

```
make install PREFIX=/usr DESTDIR=/tmp/stage
```

Rust, no dependencies, no `unsafe` code. `make` is a thin wrapper over
`cargo build --release`, which works on its own if you prefer it.

## Using it

```
tabasm --cpu <name> [options] source [object [listing [export [symbol]]]]
```

Only the source file is required; the other four default to the source's base
name with `.obj`, `.lst`, `.exp` and `.sym`.

```
tabasm --cpu z80 -g2 firmware.asm firmware.s19
```

### Where the tables are found

Each name is a file, so the assembler has to locate it. It tries, in order:

1. `$TASMTABS`, which names one directory and overrides everything else;
2. the working directory;
3. `tables/` beside the executable — an unpacked archive, or a Windows install;
4. `../share/tabasm/tables` relative to the executable, so `/usr/local/bin/tabasm`
   finds `/usr/local/share/tabasm/tables` and a Homebrew prefix works unmodified;
5. the per-user data directory — `$XDG_DATA_HOME/tabasm/tables`, or
   `~/.local/share/tabasm/tables`; `%LOCALAPPDATA%\tabasm\tables` on Windows,
   and `~/Library/Application Support/tabasm/tables` on macOS;
6. `/usr/local/share/tabasm/tables`, then `/usr/share/tabasm/tables`;
   `%ProgramFiles%\tabasm\tables` on Windows.

The first two are the whole of the original's behaviour, in its order, so
nothing that resolves today resolves differently. If no table is found, the
error lists every path tried.

To install from a release archive:

```
sudo cp tabasm /usr/local/bin/
sudo mkdir -p /usr/local/share/tabasm && sudo cp -r tables /usr/local/share/tabasm/
sudo cp doc/tabasm.1 /usr/local/share/man/man1/
```

The archive contains no build system, so `make install` is for a source
checkout rather than a download.

### Targets

| `--cpu` | Processor |
|---|---|
| `8048` | Intel 8048 |
| `6502` | MOS 6502 |
| `6800` | Motorola 6800, 6801, 68HC11 |
| `6303` | Hitachi HD6303 (6801 superset) |
| `6805` | Motorola 6805 |
| `8051` | Intel 8051 |
| `8085` | Intel 8080/8085 |
| `8096` | Intel 8096 |
| `z80` | Zilog Z80 |
| `tms7000` | TI TMS7000 |
| `tms32010` | TI TMS32010 |
| `tms320c25` | TI TMS320C25 |

Each name is the table's file name: `--cpu z80` reads `z80.tab2`. Write your own
and it is selectable the same way, with no change to the assembler.

### Output formats

`-g0` Intel HEX (default) · `-g1` MOS Technology · `-g2` Motorola S-record ·
`-g3` raw binary · `-g4` Intel HEX with word addresses. `-b` is `-g3` plus
contiguous-block output; `-c` makes any format contiguous; `-o<xx>` sets bytes
per record, in hex.

### Listing

`-l`, `-ll`, `-la` append a label table; `-h` a hex dump; `-p<n>` pages the
listing; `-q` suppresses it; `-e` shows macro-expanded source; `-s` writes a
symbol file. `-a` enables strict checks — an operand wrapped entirely in
parentheses, unused argument bytes, duplicate labels, an expression opening with a
binary operator.

## Compatibility with TASM

Three switches exist for it, and they are independent because they answer
different questions.

**`--compatibility`** restores two behaviours the original had and this one does
not. Its expression evaluator had no operator precedence — everything bound
equally, left to right, so `1+2*3+4` was 13 rather than 11. And `.UNDEF` was
listed as a directive but implemented in neither pass, so it silently kept the
macro it claimed to remove, and a later `#ifdef` took the branch the author did
not intend.

**`--bug-compatibility`** restores two defects that this assembler fixes.

- The `-g4` word-address checksum was computed from the *byte* address while the
  record printed the *word* address, so the two disagreed whenever the address was
  non-zero. Such records fail validation in any conforming Intel HEX loader:
  assembling a 17-record file that way produces 15 invalid records. By default the
  checksum matches the address printed.
- The symbol table was ordered by a shaker sort keyed on the first character
  whose bookkeeping stopped early, so for some inputs the label table came out not
  quite sorted. By default the ordering is a full lexical sort, which is what a
  symbol list is for.

**`--message-prefix`** and **`--page-title`** set the name on the assembler's own
messages and the paged-listing heading. They are values rather than a mode because
matching another tool's output is a legitimate thing to ask for and an odd thing
to hide.

So to reproduce TASM's output exactly:

```
tabasm --compatibility --bug-compatibility --message-prefix tasm --cpu 8051 x.asm
```

`tabasm --help` is not implemented; this file is the reference. `doc/` documents
both table formats.

## Testing

```
cargo test
```

Four suites, no dependencies to install:

- **unit tests** over the expression evaluator, the matcher, the encoding rules,
  the object writers and the table parsers;
- **`testing/conformance.rs`** — whether the output is *right*. It sweeps all 3028
  rows of the twelve tables, requiring each to be reachable and to encode as the
  table declares; re-derives every object checksum rather than remembering it;
  cross-checks the object file against the listing; and checks the documented
  expression values. It shares no code with `src/` on purpose;
- **`testing/golden.rs`** — whether the output is *unchanged*, over 115 recorded
  cases;
- **`testing/robustness.rs`** — hostile sources, tables, command lines and
  environment variables must not crash or hang it.

CI runs the suites on Linux, macOS and Windows, along with an overflow-checked
build, `rustfmt`, `clippy` and the minimum supported Rust version. Because the
recorded output was captured on one machine, running it on three is how the
project checks that the assembler emits identical bytes everywhere.

During development the assembler was also compared directly against the original
release, over 363 artefacts across eleven processors and twelve output variants.
That comparison needs the original binary and is not part of this repository.

## Licence

BSD 3-Clause; see `LICENSE`. The instruction encodings in `tables/` are facts
about the target processors and the tables carry no upstream text — see `NOTICE`
for why the licence applies cleanly to all of it.

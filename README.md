# tabasm

A table-driven, two-pass, absolute cross-assembler for 8- and early-16-bit
microprocessors. Twelve targets are supported out of the box, and adding another
usually means writing a table rather than changing the program.

tabasm is an independent reimplementation of the **Telemark Assembler (TASM)**,
a table-driven cross-assembler first released in 1985. It reproduces TASM's
output byte for byte where asked to, corrects two of its defects by default, and
adds a table format that says what it means.

## Prebuilt binaries

Find and download **[binaries of the latest release here](https://github.com/dg1yfe/tabasm/releases/latest)**.


## Installing

On macOS, via Homebrew:

```
brew trust dg1yfe/tap
brew install dg1yfe/tap/tabasm
```

Without `brew trust`, tapping fails with *invalid syntax in tap* — a misleading
error; the formula is fine. macOS only; on Linux take the tarball.

## Building and installing from source

```
make                           # build
sudo make install              # to /usr/local
sudo make uninstall
```

Installs the assembler to `$PREFIX/bin`, the tables to
`$PREFIX/share/tabasm/tables` where it looks for them, and the man page and docs
alongside. `PREFIX` defaults to `/usr/local`. `DESTDIR` stages the install
elsewhere, for packaging:

```
make install PREFIX=/usr DESTDIR=/tmp/stage
```

Rust, no dependencies, no `unsafe`. `make` wraps `cargo build --release`.

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

The first two are the original's entire behaviour, in its order. If no table is
found, the error lists every path tried.

To install from a release archive, which carries no build system:

```
sudo cp tabasm /usr/local/bin/
sudo mkdir -p /usr/local/share/tabasm && sudo cp -r tables /usr/local/share/tabasm/
sudo cp doc/tabasm.1 /usr/local/share/man/man1/
```

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

### Table formats

Two are read. `<name>.tab2` is the current one, described in
`doc/table-format.md`. TASM's own `tasm<name>.tab` is read directly and tried in
every directory searched, so existing tables work unchanged —
`doc/table-format-legacy.md` describes that format.

Converting is therefore optional. `tab1to2` does it if you want the current
format's comments and named rules:

```
tab1to2 tasm51.tab 8051.tab2 "Table-driven 8051 Assembler"
```

It is built from a source checkout, not shipped in the release archives.

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

To reproduce TASM's output exactly:

```
tabasm --compatibility --bug-compatibility --message-prefix tasm --cpu 8051 x.asm
```

| switch | restores |
|---|---|
| `--compatibility` | no operator precedence, so `1+2*3+4` is 13, not 11; `.UNDEF` does nothing |
| `--bug-compatibility` | the `-g4` checksum taken from the byte address while the record prints the word address; the symbol sort that stops early |
| `--message-prefix`, `--page-title` | the name on messages and the paged-listing heading |

The two defects are corrected by default: `-g4` records that disagree with their
own address fail validation in any conforming Intel HEX loader, and a symbol
list is sorted lexically.

`tabasm --help` lists the options; `man tabasm` is the full reference; `doc/`
covers both table formats. There is no `-h` for help — `-h` is the hex dump, as
in the original.

## Testing

```
cargo test
```

Four suites, nothing to install:

- **unit tests** — evaluator, matcher, encoding rules, object writers, table
  parsers;
- **`conformance.rs`** — whether the output is *right*: sweeps all 3028 table
  rows for reachability and declared encoding, re-derives every checksum, and
  cross-checks object against listing. Shares no code with `src/`, deliberately;
- **`golden.rs`** — whether the output is *unchanged*, against recorded output;
- **`robustness.rs`** — hostile sources, tables, arguments and environment must
  not crash or hang it.

CI runs all four on Linux, macOS and Windows, plus an overflow-checked build,
`rustfmt`, `clippy` and the declared minimum Rust version. The recorded output
was captured on one machine, so three platforms is how identical bytes
everywhere gets checked.

The assembler was also compared directly against the original release during
development — 363 artefacts, eleven processors, twelve output variants. That
needs the original binary and is not in this repository.

## Licence

BSD 3-Clause; see `LICENSE`. The instruction encodings in `tables/` are facts
about the target processors and the tables carry no upstream text — see `NOTICE`
for why the licence applies cleanly to all of it.

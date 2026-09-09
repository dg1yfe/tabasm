# tabasm

A table-driven, two-pass, absolute cross-assembler for 8- and early-16-bit
microprocessors. Eleven targets are supported out of the box, and adding another
usually means writing a table rather than changing the program.

tabasm is an independent reimplementation of **TASM**, a table-driven assembler
first published in the 1980s. It reproduces TASM's output byte for byte where
asked to, corrects two of its defects by default, and adds a table format that
says what it means.

## Building

```
cargo build --release          # target/release/tabasm
make -C src                    # or a binary at src/tabasm
```

Rust, no dependencies, no `unsafe` code.

## Using it

```
tabasm --cpu <name> [options] source [object [listing [export [symbol]]]]
```

Only the source file is required; the other four default to the source's base
name with `.obj`, `.lst`, `.exp` and `.sym`. Tables are found through `TASMTABS`,
which names one directory:

```
export TASMTABS=/usr/local/share/tabasm
tabasm --cpu z80 -g2 firmware.asm firmware.s19
```

### Targets

| `--cpu` | Processor |
|---|---|
| `8048` | Intel 8048 |
| `6502` | MOS 6502 |
| `6800` | Motorola 6800, 6801, 68HC11 |
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
- **`testing/conformance.rs`** — whether the output is *right*. It sweeps all 2811
  rows of the eleven tables, requiring each to be reachable and to encode as the
  table declares; re-derives every object checksum rather than remembering it;
  cross-checks the object file against the listing; and checks the documented
  expression values. It shares no code with `src/` on purpose;
- **`testing/golden.rs`** — whether the output is *unchanged*, over 115 recorded
  cases;
- **`testing/robustness.rs`** — hostile sources, tables, command lines and
  environment variables must not crash or hang it.

During development the assembler was also compared directly against the original
release, over 363 artefacts across eleven processors and twelve output variants.
That comparison needs the original binary and is not part of this repository.

## Licence

BSD 3-Clause; see `LICENSE`. The instruction encodings in `tables/` are facts
about the target processors and the tables carry no upstream text — see `NOTICE`
for why the licence applies cleanly to all of it.

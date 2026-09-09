# The instruction table format

An instruction table describes one processor's instruction set. It is plain text,
read once at startup, and selected by name: `--cpu z80` reads `z80.tab2` from the
directory `TASMTABS` names, or from the working directory if it is unset.

Everything the assembler knows about a processor lives here. Only a genuinely new
*encoding rule* — a way of folding operand values into instruction bytes —
requires a change to the program.

## File structure

Three kinds of line, distinguished by their first character:

| Starts with | Is |
|---|---|
| `;` | a comment, to end of line; also valid after a directive or a row |
| `%` | a directive |
| anything else | an instruction row |

Blank lines are ignored. A comment may appear anywhere, and `;` is the leader
rather than `#` because `#` is the immediate-addressing prefix in many operand
patterns.

## Directives

```
%format         2
%banner         "Table-driven Z80 Assembler"
%opcode-order   ls-first
%address-unit   byte
%aux-registers  2
%class          1 z80
%class          2 hd64180
%regset         "*BR0+" mask=F0 class=1
%columns        mnemonic operands opcode op-bytes arg-bytes rule
```

| Directive | Meaning |
|---|---|
| `%format 2` | Required, and must come before any row. |
| `%banner "…"` | The processor's label, shown in the paged-listing heading. |
| `%opcode-order ls-first \| ms-first` | Byte order for multi-byte **opcodes**. Argument bytes are always least-significant first; a rule that needs otherwise swaps the value. |
| `%address-unit byte \| word` | With `word` the counter counts 16-bit words: `$` and `*` yield word addresses, the image stays byte-indexed, and an odd byte count still advances a whole word. |
| `%aux-registers <n>` | How many auxiliary registers the target has, a power of two. Read by the `tms-aux` rule. |
| `%class <hex> <name>` | Names a class bit, so a row may say `class=hd64180` instead of `class=2`. |
| `%regset "<name>" mask=<hex> [class=<hex>]` | One register-set entry. A `<reg>` placeholder matches any entry's name; the longest match wins, so declaration order does not matter. At most 32 entries. |
| `%columns …` | Required. Declares the row layout, and a row not matching it is an error rather than a silent shift. |

## Instruction rows

```
mnemonic  operands            opcode  op-bytes  arg-bytes  rule        [parameters]

ACALL     <expr>              11      1         1          jmp-page-2k  page-mask=F800
ADD       A,#<expr>           24      1         1          plain
ADD       A,R0                28      1         0          plain
ANL       C,/<expr>           B0      1         1          plain
CJNE      A,#<expr>,<expr>    B4      1         2          combine-rel
NOP       -                   00      1         0          plain
```

The six columns may be declared in any order; these are their meanings.

- **mnemonic** — matched against the source case-insensitively.
- **operands** — the pattern. Every character is a literal except the two
  placeholders below. `-` means the instruction takes no operands.
  - `<expr>` captures an expression. Every capture but the last must be followed
    by a literal — `,`, `[` or `]` — so its extent is unambiguous; the last runs to
    the end of the operand text.
  - `<reg>` matches a `%regset` name and folds that entry's mask into the opcode.
- **opcode** — hex digits, one to four bytes' worth. `op-bytes` must agree with the
  digit count.
- **op-bytes**, **arg-bytes** — how many bytes of opcode and of argument the
  instruction emits. Both are stated; neither is inferred.
- **rule** — the encoding rule, by full name (below).
- **parameters** — trailing `key=value` pairs. `class=` is accepted on any row and
  defaults to `1`; the rest are per-rule.

**Row order is significant.** Matching takes the first row whose mnemonic, class
and operand pattern all fit, so specific patterns must precede general ones: with
`ADD A,#<expr>` after `ADD A,<expr>`, an immediate operand would match the general
row and capture the `#` along with the value. A row repeating an earlier pattern
for the same mnemonic can never match and is rejected as unreachable.

## Universal parameters

| Parameter | Meaning |
|---|---|
| `class=<hex\|name>` | Which instruction-set variants the row belongs to. A row participates when `class & mask` is non-zero, the mask coming from `-x`; without `-x` the mask is 1. |
| `post-shift=<hex>`, `post-or=<hex>` | A transform applied to the argument value after the rule has run: `value = (value << post-shift) \| post-or`. |

## Encoding rules

Rules the twelve shipped tables use are marked; the rest are implemented and
selectable, but nothing here exercises them.

| Rule | Parameters | Used by a shipped table |
|---|---|---|
| `plain` | — | yes |
| `jmp-page-2k` | `page-mask` | yes |
| `jmp-page-256` | — | yes |
| `rel8` | — | yes |
| `zero-page` | — | yes |
| `zero-page-moto` | — | yes |
| `bit-moto` | — | yes |
| `bit-z80` | — | yes |
| `index-z80` | — | yes |
| `combine` | — | yes |
| `combine-rel` | — | yes |
| `combine-swapped` | `valid-mask` | yes |
| `swap-bytes` | — | yes |
| `three-rel` | — | yes |
| `tms-fold` | `shift`, `invert`, `valid-mask` | yes |
| `tms-dma` | `shift`, `invert`, `valid-mask` | yes |
| `tms-long` | `shift`, `invert`, `valid-mask` | yes |
| `tms-long-swapped` | `shift`, `invert`, `valid-mask` | yes |
| `tms-aux` | `shift`, `invert`, `valid-mask` | yes |
| `tms7000-trap` | — | yes |
| `rel16` | — | yes |
| `i8096-combine` | `field-mask`, `set-bits` | yes |
| `i8096-short-long-2` | `field-mask`, `opcode-xor` | yes |
| `i8096-short-long-3` | `field-mask` | yes |
| `i8096-jump-bit` | `field-mask` | yes |
| `i8096-rel11` | `field-mask` | yes |
| `i8096-indexed` | `field-mask` | yes |
| `i8096-short-long-1` | `field-mask` | yes |
| `i8096-combine-swapped` | `field-mask` | yes |
| `tms9900-swap-dst` | — | no |
| `tms9900-swap-src` | — | no |
| `tms9900-regs` | — | no |
| `nibble-dma` | `shift`, `invert`, `valid-mask` | no |
| `three-plain` | — | no |
| `z8-nibbles` | `valid-mask` | no |
| `z8-nibbles-swapped` | `valid-mask` | no |
| `zero-page-st7` | — | no |
| `bit-st7` | — | no |
| `rel4` | — | no |
| `z8-working-pair` | — | no |
| `z8-djnz` | — | no |
| `z8-load-indexed` | — | no |

Per-rule parameters, all hexadecimal:

| Parameter | Read by | Meaning |
|---|---|---|
| `page-mask` | `jmp-page-2k` | Which high bits of the target must match the counter's. Zero disables the check, for a processor that takes its high address bits from elsewhere. |
| `valid-mask` | the `tms-*` rules, `combine-swapped`, `z8-nibbles*` | Bits the folded value may occupy. A value with bits outside it is out of range. |
| `field-mask` | the `i8096-*` rules | As `valid-mask`, but sliced per operand: each field is checked against its own portion. |
| `shift` | the `tms-*` rules | How far left to shift the operand before masking, 0–15. |
| `invert=yes\|no` | the `tms-*` rules | Invert the masked bit field, for an operand that counts from the opposite end. |
| `set-bits` | `i8096-combine` | Bits OR-ed into the first argument byte unconditionally, selecting the auto-increment addressing modes. |
| `opcode-xor` | `i8096-short-long-2` | XOR-ed into the opcode when the instruction shortens, for the two mnemonics that do not follow the family's opcode pattern. |

## What the loader rejects

Errors name the line and what was wrong:

- a row whose column count does not match `%columns`, or an unrecognised trailing
  field;
- `op-bytes` disagreeing with the opcode's digit count;
- `arg-bytes` above 8, or `op-bytes` outside 1–4;
- an unknown rule name, or a parameter the named rule does not read;
- a non-final `<expr>` with no following literal to end it;
- an unknown placeholder, directive, or class name;
- a row repeating an earlier pattern for the same mnemonic;
- more than 1200 rows, or more than 32 register-set entries.

## Converting a legacy table

The assembler also reads the older positional format, described in
`table-format-legacy.md`. `tab1to2` rewrites one in this format:

```
tab1to2 tasm51.tab 8051.tab2 "Table-driven 8051 Assembler"
```

The legacy format has no structure for commentary, so the conversion keeps only
the structural data and drops every comment line.

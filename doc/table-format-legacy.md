# The legacy instruction table format

tabasm reads two table formats. This is the older one, used by TASM, kept because
existing tables are written in it. The current format is described in
`table-format.md`; `tab1to2` converts between them.

The format is chosen by the file's **content**, not its name: a table whose first
substantive line is `%format` is read as the current format, anything else as this
one. A given `--cpu <name>` is looked for as `<name>.tab2` first and then as
`tasm<name>.tab`.

## File structure

Lines are dispatched on their **first character only**:

| First character | Line is |
|---|---|
| `"` on line 1 | the banner |
| `.` | a directive |
| `A`–`Z` | an instruction row |
| anything else | ignored, silently |

There is **no comment syntax**. Lines beginning `/*` or `;` are skipped because
they begin with neither an upper-case letter nor a `.`, not because they are
recognised as comments — so `/*` opens nothing and needs no `*/` to close it, and
a `/*` inside a row is data. `tasm51.tab` relies on this: `ANL C,/*` uses `/` as
the 8051 complement-bit operator and `*` as a wildcard.

Two consequences to know when editing such a table:

- an instruction row must start in **column 1**; indent it and it vanishes with no
  diagnostic;
- the first character must be **upper case**. `NOP` is a row, `nop` is not.

## Banner

Line 1, in double quotes. Only the text between the first and second quote is
used; trailing spaces inside them are padding.

```
"TASM 8051 Assembler.    "
```

## Directives

| Directive | Meaning |
|---|---|
| `.MSFIRST` | Multi-byte opcodes emit most-significant byte first. |
| `.LSFIRST` | Least-significant first; the default. |
| `.WORDADDRS` | The counter counts 16-bit words rather than bytes. |
| `.NOARGSHIFT` | Suppress the implicit post-rule transform, below. |
| `.ALTWILD[c]` | Change the wildcard from `*` to `c`, or to `@` if none is given. The TMS320 tables need this because `*` is their indirect-addressing syntax. |
| `.REGSET <name> <mask> <class>` | One register-set entry. |

`.REGSET` **order matters**: a `!` in an operand pattern matches by prefix, taking
the first entry that fits, so a shorter name must be declared after any longer one
it prefixes. `*BR0+` has to precede `*0+`.

## Instruction rows

Whitespace-separated, **positional**:

```
INSTRUCTION  ARGS  OPCODE  NBYTES  RULE  CLASS  [SHIFT]  [OR]

ACALL *       11 2 JMP 1 0 F800
ADD  A,#*     24 2 NOP 1
NOP  ""       00 1 NOP 1
```

- **ARGS** — the operand pattern. `*` (or the `.ALTWILD` character) is a wildcard
  capturing an expression; `!` matches a register-set name; `""` means no operands;
  every other character is a literal. Each wildcard but the last must be followed by
  `,`, `[` or `]`.
- **OPCODE** — hex digits. **The byte count is the digit count divided by two**, and
  is never written down: `24` is one byte, `21DD` is two. A one-byte zero opcode
  must therefore be written `00`, not `0`.
- **NBYTES** — the instruction's total length. **The argument byte count is derived**
  as `NBYTES` minus the opcode's byte count. A row whose `NBYTES` is smaller than
  its opcode is rejected.
- **RULE** — the encoding rule, keyed on its **first two characters only**. `NOP` and
  `NOTOUCH` therefore select the same rule, as do `COMB` and `COMBINE`; and
  `COMBREL` selects `CO`, the combine rule, rather than the `CR` rule its spelling
  suggests. The alias for `CR` is `CREL`.
- **CLASS** — a hex bit mask of instruction-set variants, matched against `-x`.
- **SHIFT**, **OR** — optional and hexadecimal. `OR` may appear only if `SHIFT`
  does. Both are omitted by most rows.

Because the fields are positional and the last two optional, a missing column
shifts every later one without complaint. A row is also indistinguishable from one
with trailing commentary, so a seventh field is read as `SHIFT` only if it parses
as hexadecimal — `;8041` after `CLASS` is ignored, as several rows in
`tasm48.tab` rely on.

**Row order is significant**, exactly as in the current format: the first matching
row wins, so `ADD A,#*` must precede `ADD A,*`.

## SHIFT and OR

These two columns carry different things depending on the rule, and on whether the
table declared `.NOARGSHIFT`. Reading a table means knowing which applies.

Unless `.NOARGSHIFT` is given, an implicit transform runs after the rule:

```
argument = (argument << SHIFT) | OR
```

So in a table without `.NOARGSHIFT`, `OR` is a value to be OR-ed into the
argument. In a table with it, the columns are the rule's to interpret:

| Rule | `SHIFT` | `OR` |
|---|---|---|
| `JMP` | — | mask of high bits that must agree; zero disables the check |
| `T1`, `TDMA`, `TLK`, `T5`, `TAR` | shift count in the low nibble; a non-zero **high** nibble inverts the masked field | validity mask |
| `CSWAP` | — | validity mask |
| `I1` | bits OR-ed into the first argument byte | per-field validity mask |
| `I2` | XOR-ed into the opcode when the instruction shortens | per-field validity mask |
| `I3`–`I8` | — | per-field validity mask |

The auxiliary-register width of the TMS320 rules is not in the table at all: the
original decided it by comparing the `-<nn>` selector text against `3225`. When
loading a legacy table tabasm derives it from the selector the same way, so a
table copied to another name behaves differently. The current format states it
outright as `%aux-registers`.

## Limits

At most 1200 instruction rows and 32 register-set entries; exceeding either is
fatal.

## Converting

```
tab1to2 <in.tab> <out.tab2> [banner]
```

Structural data is carried over and commentary is dropped, since the format has
nowhere to put it. Supplying a banner replaces the table's own.

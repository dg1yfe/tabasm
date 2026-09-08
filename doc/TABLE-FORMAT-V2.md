# A proposed instruction-table format

the specification §11.4 sketches this: "A cleaner native format —
named fields, an explicit argument-byte count, un-overloaded columns — is worth
having, but it belongs in Phase 2: introduce it alongside a converter from the
original format, so the Phase-1 tables and their vector coverage are not lost."

This is that proposal, written after implementing the v1 loader in full. Every
flaw below is one the implementation actually had to work around, not a
theoretical objection.

---

## 1. What is wrong with v1

### 1.1 Positional fields with no delimiters

A row is `INSTRUCTION ARGS OPCODE NBYTES RULE CLASS [SHIFT] [OR]`. Omit a column
and every later one shifts silently. This is not hypothetical: it is the
`IM0`/`IM1`/`IM2` defect of §7.19, where three Z80 rows lacking the `ARGS` column
registered an operand pattern of `46ED`, an opcode of `0x02`, and a byte count
parsed from the text `NOP`. Nothing was diagnosed; the rows simply never matched.

Because `SHIFT` and `OR` are optional *and* positional, the loader cannot tell a
real seventh field from trailing commentary. Thirteen rows in `tasm48.tab` carry
`;8041` there. The rule this implementation had to invent — *field 7 is `SHIFT`
only if it parses as hexadecimal* — is a heuristic over a format that should not
need one.

### 1.2 There is no comment syntax

A line is an instruction row if and only if its first character is an upper-case
`A`–`Z`; everything else is skipped in silence. So:

- `/*` and `;` work as comment leaders **by accident**, not by design;
- an accidentally indented row vanishes with no diagnostic;
- `/*` cannot be treated as a comment, because `tasm51.tab` uses `/` as the 8051
  complement-bit operator inside an operand pattern: `ANL C,/*`. Strip `/*` as a
  comment and you delete that row and some four hundred in `tasm80.tab`.

### 1.3 Rule names are keyed on two characters

`NOP` ≡ `NOTOUCH`, `COMB` ≡ `COMBINE`, `CR` ≡ `CREL` — all harmless. But
`COMBREL` keys as `CO` and silently selects COMBINE rather than the CR rule its
author plainly intended. A misspelling that changes the encoding and is never
reported.

### 1.4 The argument-byte count is derived, not stated

`arg_bytes = NBYTES − (opcode hex digits ÷ 2)`. Two consequences: the subtraction
underflows on a malformed row and must be guarded (unguarded it drove a runaway
emission loop in the original), and a one-byte opcode of zero must be written
`00`, never `0`, because the *digit count* carries the width.

### 1.5 `SHIFT` and `OR` mean different things per rule — and per table

This is the worst of it. The same two columns carry at least five distinct
meanings, and which one applies depends on the rule *and* on a table-level
directive:

```
tasm80.tab    BIT  *,(IX*)  CBDD 4 ZBIT 1 0 4600     <- 4600 is a VALUE OR-ed into argval
tasm3210.tab  ADD  !,@,@    0000 2 T1   1 8 0F00     <- 0F00 is a VALIDATION MASK
                                          ^ and this 8 is an INVERT FLAG, not a shift count
```

`tasm80.tab` does not declare `.NOARGSHIFT`, so the implicit
`argval = (argval << SHIFT) | OR` step runs and `OR` is a value. `tasm3210.tab`
does declare it, so the step is suppressed and the same column is a mask consumed
by the rule. A reader cannot tell which without checking a directive fifty lines
up. The full inventory:

| Column | Meaning | Selected by |
|---|---|---|
| `OR` | value OR-ed into `argval` | the default step, i.e. no `.NOARGSHIFT` |
| `OR` | mask of high bits that must agree | `JM` |
| `OR` | validity mask | `CS`, and `T1`/`TD`/`TL`/`T5`/`TA` via `shift_and` |
| `OR` | per-field validity mask | `I1`–`I8` via `isargvalid` |
| `SHIFT` | shift count in the low nibble, **invert flag in the high nibble** | `shift_and` |
| `SHIFT` | unconditional OR into `argval` | `I1` (auto-increment) |
| `SHIFT` | XOR against the opcode | `I2` (`XCH`/`XCHB`) |

### 1.6 Significant order that looks incidental

Row order *is* the language: first match wins, so `ADD A,#*` must precede
`ADD A,*` or an immediate operand never reaches the immediate row. Nothing marks
this, and tidying a table alphabetically would change what it accepts.
`.REGSET` order is load-bearing for the same reason — matching is by prefix, so
`*BR0+` must be declared before `*0+`.

### 1.7 One behaviour cannot be expressed at all

The TMS auxiliary-register width is decided by string-comparing the `-<nn>`
selector against `"3225"` (§7.20). Copy `tasm3225.tab` to `tasm9999.tab` and the
identical bytes accept a narrower range. No `.tab` file can state this, so a
reimplementation must special-case a processor name.

---

## 2. Design goals

1. **No silent misparse.** Every row is checked against a declared column list; a
   missing or extra field is an error naming the line.
2. **Nothing derived that could be stated.** Byte counts are written down.
3. **One meaning per field.** Rule parameters are named, and a rule accepts only
   the parameters it defines.
4. **Names, not prefixes.** Rules are matched in full; an unknown name is an
   error rather than a silent neighbour.
5. **Order-dependence explicit and checkable.** Still first-match, but the loader
   can now detect an unreachable row.
6. **Express what v1 could not**, so no behaviour lives in the program's
   knowledge of processor names.

---

## 3. The format

Directives begin with `%`, comments with `;`, and a row is anything else. Both
are unambiguous because they are declared rather than inferred from the first
character.

**The comment leader is `;`, not `#`.** `#` is the immediate-addressing prefix in
162 operand patterns across the shipped tables, so using it would truncate
`ADD A,#<expr>` to `ADD` — the same class of mistake v1 makes with `/*`. Of the
characters that never appear in any operand pattern, `;` is the one an assembly
programmer expects. (`<` is likewise absent from every pattern, which is what
makes `<expr>` and `<reg>` safe as placeholders; `>` does appear, twelve times,
and stays a literal.)

```
; tasm51.tab2 — Intel 8051
%format         2
%banner         "TASM 8051 Assembler.    "
%opcode-order   ls-first          ; was .LSFIRST / .MSFIRST
%address-unit   byte              ; was .WORDADDRS (unit=word)
%class          1 base            ; CLASS bits get names
%columns        mnemonic operands opcode op-bytes arg-bytes rule

ACALL   <expr>              11    1 1  jmp-page-2k  page-mask=F800
ADD     A,#<expr>           24    1 1  plain
ADD     A,<expr>            25    1 1  plain
ADD     A,R0                28    1 0  plain
ANL     C,/<expr>           B0    1 1  plain
CJNE    A,#<expr>,<expr>    B4    1 2  combine-rel
LCALL   <expr>              12    1 2  swap-bytes
NOP     -                   00    1 0  plain
```

Four changes carry most of the value.

**`<expr>` and `<reg>` replace `*` and `!`.** Everything else in an operand
pattern is a literal, so `.ALTWILD` disappears entirely — `*` is just a character
now, which is what the TMS320 tables needed it to be:

```
; tasm3225.tab2 — TMS320C25
%opcode-order   ms-first
%address-unit   word
%aux-registers  8                 ; §1.7: no longer a special case in the program
%regset         "*BR0+" mask=F0 class=1
%regset         "*0+"   mask=E0 class=1

ADD     <reg>,<expr>,<expr>  0088  2 0  tms-fold  shift=8 valid-mask=0F00
ADD     <expr>               0000  2 0  tms-fold  valid-mask=007F
LAR     <expr>,<reg>,<expr>  3088  2 0  tms-aux   valid-mask=07
```

The `8` in v1's `SHIFT` column here is a shift **count**, so it becomes
`shift=8`. §7.20's invert flag lives in that column's **high** nibble, and no
shipped table sets it: the five distinct `SHIFT` values in the whole corpus are
`0`, `00`, `01`, `8` and `0C`, every one with a zero high nibble. `invert=yes`
exists in v2 for completeness and is exercised by nothing.

`%regset` order stops being load-bearing: the loader matches the **longest**
name, not the first declared.

**Byte counts are stated, not derived.** `op-bytes` and `arg-bytes` are both
explicit, so nothing underflows and `00` versus `0` no longer encodes a width.
The loader checks `op-bytes` against the opcode's digit count and errors on
disagreement — the v1 hazard becomes a diagnostic.

**Rule parameters are named, and the implicit step is gone.** There is no
table-level `.NOARGSHIFT`; the default transform is written per row where it
applies, so the tasm80/tasm3210 collision of §1.5 cannot recur:

| v1 | v2 |
|---|---|
| `SHIFT`/`OR` under the default step | `post-shift=`, `post-or=` |
| `OR` as an agreement mask (`JM`) | `page-mask=` |
| `OR` as a validity mask | `valid-mask=` |
| `OR` as a per-field validity mask | `field-mask=` |
| `SHIFT` low nibble as a shift count | `shift=` |
| `SHIFT` high nibble as invert flag (unexercised) | `invert=yes` |
| `SHIFT` as auto-increment OR (`I1`) | `set-bits=` |
| `SHIFT` as opcode XOR (`I2`) | `opcode-xor=` |

**Rules are named in full.** `plain`, `jmp-page-2k`, `jmp-page-256`, `rel8`,
`rel16`, `zero-page`, `zero-page-moto`, `bit-moto`, `bit-z80`, `index-z80`,
`combine`, `combine-rel`, `combine-swapped`, `swap-bytes`, `three-rel`, and for
the later families `tms-fold`, `tms-dma`, `tms-long`, `tms-long-swapped`,
`tms-aux`, `tms7000-trap`, and `i8096-*`. `combrel` is now an error, not a silent
`combine`.

---

## 4. What the loader can check that v1 could not

- a row with the wrong number of columns, naming the line;
- `op-bytes` disagreeing with the opcode width;
- `arg-bytes` that no rule can consume;
- an unknown rule name, or a parameter a rule does not define;
- a non-final `<expr>` not followed by a literal delimiter — the v1 rule of §5.4
  that otherwise mis-parses in silence;
- an **unreachable row**: a later pattern subsumed by an earlier one for the same
  mnemonic. This finds nothing in the shipped tables, which is the point — it
  protects whoever edits them next.

---

## 5. Migration

The converter is the whole job, and this project has an unusually strong oracle
for it. The harness pins 128 cases byte-for-byte and the reference comparison
pins 363 artefacts against the 2001 binary. So:

1. Write `tab1-to-tab2`, converting all eleven shipped tables mechanically.
2. Teach the loader v2 alongside v1, dispatching on `%format`.
3. Assemble the corpus from the converted tables. **Both suites must stay green.**
   Any difference is a converter bug, isolated to the tables, with the engine
   unchanged as the control.
4. Keep the v1 tables and loader as the fixture §11.3 asks for.

Two conversions need judgement rather than mechanism, and both are worth
recording in the findings log:

- `%aux-registers` cannot be read from a v1 table — it comes from the selector
  name. Seed it 2 for `tasm3210`, 8 for `tasm3225`.
- 120 rows carry an explicit zero `SHIFT`/`OR` pair that means nothing. The
  converter should drop them rather than emit `post-shift=0 post-or=0`.

---

## 6. What deliberately does not change

The **encoding rules themselves**, and therefore the engine. v2 renames the
parameters a rule reads; it does not alter what any rule computes. The
instruction encodings are facts about the processors and survive verbatim.
First-match-wins row semantics also stay — the change is that the loader can now
warn when order matters and a row is dead.

This is a format migration, not a behavioural one, which is why the vectors can
serve as its proof.

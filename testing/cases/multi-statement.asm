; `\` as a statement separator, and what it means inside a macro.
;
; The manual calls the backslash a newline: the rest of the line is processed
; as an independent statement. Coupled with DEFINE that is how a multiple
; statement macro is written, so the body must keep its backslashes and be
; split where it is EXPANDED, not where it is defined. Splitting first
; truncates the body and assembles the tail on the spot -- which emits bytes
; from a directive that emits none.
;
; Each statement is also listed on its own line, at its own address, and the
; bytes are counted per statement: a running total would place the second
; statement wrongly and advance the counter twice over.
        .org  0

; Two statements on one plain source line.
        ldab  #5 \ nop

; A macro whose body is two statements.
#define TWO   ldab #6 \ nop
        TWO

; DEFCONT extends the macro DEFINE last touched. Redefining a macro leaves it
; where it already sits in the table, and on the second pass every macro
; already exists -- so keying on table order appends the continuation to
; whichever macro happens to be last. LATER is defined after PAIR precisely to
; catch that: without it the bug is invisible.
#define PAIR  ldab #7
#defcont      \ nop
#define LATER nop
        PAIR
        LATER

        .end

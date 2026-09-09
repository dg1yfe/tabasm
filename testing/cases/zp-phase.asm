; Pass-1/pass-2 size disagreement and the phase error it produces.
; Assemble with -65 (6502).
;
; An undefined symbol evaluates to 0x20000, above the address space, so the
; zero-page rule declines to shorten in pass 1 and reserves three bytes. In
; pass 2 the symbol resolves to a zero-page address and only two are emitted,
; so a label following the instruction keeps its pass-1 value while the counter
; has moved on. The mismatch is reported as "label value misalligned."
;
        .org  1000h

; Baseline: already defined, shortens in both passes, no disagreement.
back    .equ  20h
        lda   back

; Forward reference to a non-zero-page address: the sentinel makes pass 1
; reserve the long form, and pass 2 agrees. No error.
        lda   faraway

; Forward reference to a zero-page address: the passes disagree.
        lda   fwd
after   nop
fwd     .equ  21h
faraway .equ  1234h

; Emits the pass-1 value of "after", not the address the nop occupies.
        .byte after
        .end

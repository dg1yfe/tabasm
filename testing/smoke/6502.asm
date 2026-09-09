; 6502 smoke test.
;
; Short on purpose: a handful of instructions that need no operand evaluation,
; every data directive, a label and an equate. Its value is breadth across the
; targets and the object formats, not depth -- testing/conformance.rs covers
; every table row.
        .org  0
val     .equ  $12
        .byte 1,2,3
        .word $1234
        .text "smoke"
        ASL   A
        BRK   
        CLC   
        CLD   
        CLI   
        CLV   
        DEX   
        DEY   
here:   .byte 4
        .block 4
        .byte val
        .end

; HD6303 smoke test.
;
; Short on purpose: a handful of instructions that need no operand evaluation,
; every data directive, a label and an equate. Its value is breadth across the
; targets and the object formats, not depth -- testing/conformance.rs covers
; every table row.
;
; The last four are what the HD6303 adds to the 6801; MUL and ABX are the
; 6801's own additions to the 6800, which this table also carries.
        .org  0
val     .equ  $12
        .byte 1,2,3
        .word $1234
        .text "smoke"
        ABA   
        ASLA  
        CBA   
        CLC   
        ABX   
        MUL   
        XGDX  
        SLP   
        AIM   #$0F,val
        OIM   #$80,val
        EIM   #$01,val
        TIM   #$10,val
here:   .byte 4
        .block 4
        .byte val
        .end

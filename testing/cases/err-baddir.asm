; A directive that does not exist, plus a malformed operand to a real one.
; Exercises the "bad directive" / "bad argument" diagnostics.
        .org  0
        .notadirective
        .byte
        nop
        .end

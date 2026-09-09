; Mnemonics that appear in no instruction table.
; Exercises the "bad instruction" diagnostic and the table-miss path.
        .org  0
        frobnicate a,r0
        nop
        wibble
        .end

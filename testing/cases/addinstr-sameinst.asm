; same_inst matching across mnemonics that share a long prefix.
; Assemble with -51.
;
; The loader records the previous row's mnemonic to detect consecutive rows of
; the same instruction (the same_inst optimisation). That record used to be a
; 10-byte buffer written truncated but compared in full, so two DIFFERENT
; mnemonics sharing a 9-character prefix compared equal: the second was then
; treated as an operand variant of the first, encoded with the wrong opcode,
; and could never be matched on its own.
;
; Here ABCDEFGHIJ (10 chars) and ABCDEFGHI (9 chars) share the 9-char prefix
; that the old buffer truncated to. Each must keep its own opcode: 11 and 22.
; Documented in the notes.
        .org  0

        .addinstr ABCDEFGHIJ A 11 1 NO 1
        .addinstr ABCDEFGHI  A 22 1 NO 1

        ABCDEFGHIJ A            ; must emit 11
        ABCDEFGHI  A            ; must emit 22, not 11, and not error

        .end

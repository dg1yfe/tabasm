; The IM0/IM1/IM2 alternate spellings in tasm80.tab.
; Assemble with -80 -x (Z80).
;
; tasm80.tab offers two spellings of the interrupt-mode instruction: "IM 0"
; with a space, and "IM0" without. The no-space rows were written without the
; ARGS column, so every field shifted one place -- the operand pattern became
; the opcode text, the opcode became the byte count, and so on -- and the rows
; could never match. "im0" was rejected as an unknown instruction and emitted
; nothing, while the opcodes those rows carried were correct all along.
;
; The rows have been repaired by supplying the empty operand pattern. Both
; spellings now assemble to the same, correct encodings.
;
        .org  0

; Spaced form: always worked.
        im    0                 ; ED 46
        im    1                 ; ED 56
        im    2                 ; ED 5E

; No-space form: dead until the table was repaired, now identical to the above.
        im0                     ; ED 46
        im1                     ; ED 56
        im2                     ; ED 5E

        nop
        .end

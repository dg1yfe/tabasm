; The four strict-checking (-a) diagnostics, spec 1.4. A bare -a turns on all
; four bits, so this one source exercises each once and pins the exact wording
; and detail. Verified byte-for-byte against the 2001 release binary.
;   0x01  Invalid operand.  No indirection for this instruction.  (note 2 spaces)
;   0x02  Unused data in MS byte of argument.  -- detail is the discarded high
;         part in LOWER-case hex ($0AB00 with a 1-byte slot leaves ab)
;   0x04  Duplicate label:
;   0x08  Non-unary operator at beginning of expression.
        .org 0
dup     .equ 1
dup     .equ 2            ; 0x04  Duplicate label: (dup)
        ADD  A,#0AB00h    ; 0x02  Unused data in MS byte of argument. (ab)
        LCALL (1234h)     ; 0x01  Invalid operand.  No indirection ... ((1234h))
        LCALL %101        ; 0x08  Non-unary operator at beginning ... (%101)
        .end

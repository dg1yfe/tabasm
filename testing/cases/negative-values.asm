; Value width. Assemble with -51 -l.
;
; Expression values, label values and the program counter are 32-bit, matching
; the original targets, and are pinned to that width rather than following the
; host's "long".
;
; This is invisible in emitted object code, which masks to 8 or 16 bits, and
; visible only where a full value is printed -- which in practice means the
; label table produced by -l. An LP64 build that let these follow "long" listed
; a negative label as FFFFFFFFFFFFFFF9 where the original produced FFFFFFF9.
; That was the single difference across 363 artefacts compared against the 2001
; release binary; see the reference comparison.
;
        .org  0

neg1    .equ  -1                ; FFFFFFFF
neg7    .equ  -7                ; FFFFFFF9
negbig  .equ  -65536            ; FFFF0000
notzero .equ  ~0                ; FFFFFFFF -- complement, not negation
shifted .equ  1<<31             ; 80000000 -- sign bit of a 32-bit value
pos     .equ  32767             ; 00007FFF

; Emission masks, so the object bytes are unaffected by the width.
        .byte neg7              ; F9
        .word neg7              ; F9 FF
        .byte notzero           ; FF

        .end

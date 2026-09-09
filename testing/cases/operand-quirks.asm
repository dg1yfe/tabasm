; Two things about operand text that only real source tends to reach.
;
; Whitespace is stripped from an operand before it is matched -- but not
; inside a quoted character, where the space IS the value. Closing up the gap
; would leave the quote itself as the character.
;
; `%` introduces a binary constant. Where no binary digit follows it, the
; prefix contributes nothing and the value after it is taken instead. That is
; what makes `#%NAME` work when NAME is a macro expanding to a parenthesised
; expression rather than to digits.
        .org  0
#define BIT   (1 << 2)
#define BITS  0101

        ldab  #' '              ; the space, $20
        ldab  #'A'
        ldab  #%BITS            ; binary 0101 = 5
        ldab  #%BIT             ; the % contributes nothing: (1 << 2) = 4
        ldab  #BIT
        .byte %101, %(1 << 2), %5
        .end

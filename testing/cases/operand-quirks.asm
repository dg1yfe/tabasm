; Operand text: whitespace, and a radix prefix with nothing to work on.
;
; Whitespace is removed from an operand before it is evaluated, so `.byte 3 4`
; is the one byte 0x22 -- but not inside a quoted character, where the space IS
; the value. Closing up that gap would leave the quote as the character.
;
; `%` introduces a binary constant, and is not symmetric with `$`: a `$` with no
; hex digit is the counter, while a `%` with no binary digit is an empty binary
; constant. By default that is diagnosed and yields 0. `--compatibility`
; restores the original, where the prefix contributes nothing and the value
; after it is taken -- which is what makes `#%NAME` work when NAME is a macro
; expanding to a parenthesised expression. This case is assembled both ways.
        .org  0
#define BIT   (1 << 2)
#define BITS  0101

        ldab  #' '              ; the space, $20
        ldab  #'A'
        ldab  #%BITS            ; binary 0101 = 5, a real binary constant
        ldab  #BIT
        .byte 3 4               ; whitespace removed: one byte, 0x22
        .byte 1+2 4             ; likewise 1+24 = 25
        .byte "a b"             ; kept inside a string
        ldab  #%BIT             ; no binary digit: diagnosed by default
        .byte %5                ; likewise
        .end

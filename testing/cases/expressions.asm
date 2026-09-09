; Expression evaluation, pinned.
; Assemble with -51.
;
; This case runs in the DEFAULT mode, which applies conventional operator
; precedence. tests/cases/expressions-compat.asm is the same source assembled
; with --compatibility, which restores the original left-to-right evaluation.
; Comparing the two golden listings shows exactly what the switch changes.
;
        .org  0

; --- evaluation order -------------------------------------------------------
; Conventional precedence: * binds tighter than +.
; Under --compatibility these are 13, 21 and 20 instead.
        .byte 1+2*3+4           ; 11  = 1 + 6 + 4      (13 left-to-right)
        .byte 1+2*(3+4)         ; 15  = 1 + 14         (21 left-to-right)
        .byte 2+3*4             ; 14  = 2 + 12         (20 left-to-right)
        .byte 10-2-3            ; 5   left associative, same in both modes

; --- radix prefixes ---------------------------------------------------------
        .byte $1F               ; 31  hex, $ followed by a hex digit
        .byte %1010             ; 10  binary
        .byte @17               ; 15  octal

; --- radix suffixes ---------------------------------------------------------
; A suffixed constant must begin with a digit, hence the leading zero on 0FFh.
        .byte 0FFh              ; 255 hex
        .byte 1010b             ; 10  binary
        .byte 100d              ; 100 decimal
        .byte 0107q             ; 71  octal

; --- character constant -----------------------------------------------------
        .byte 'A'               ; 65  single quotes, exactly one character

; --- operators --------------------------------------------------------------
        .byte 5>3               ; 1   comparisons yield 0 or 1
        .byte 5<3               ; 0
        .byte 5==5              ; 1
        .byte 6&3               ; 2   bitwise and
        .byte 4|1               ; 5   bitwise or
        .byte 6^3               ; 5   bitwise xor
        .byte 1<<4              ; 16  shift left
        .byte 32>>2             ; 8   shift right
        .byte 17%5              ; 2   modulo, since a value precedes it
        .byte ~0                ; FF  bitwise not, low byte
        .byte !0                ; 1   logical not
        .byte -1                ; FF  leading minus works via a zero accumulator

; --- the program counter ----------------------------------------------------
; Both $ and * denote the counter at the start of the current statement.
        .byte $                 ; address of this byte
        .byte *                 ; address of this byte

        .end

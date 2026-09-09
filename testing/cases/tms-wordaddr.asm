; Word addressing, from the .WORDADDRS table directive.
; Assemble with -3210.
;
; The program counter counts 16-bit words rather than bytes, so:
;
;   byte address = pc * 2
;   advance(n)   = (n + 1) / 2      rounded UP
;
; An odd byte count therefore still advances a whole word, and $ and * yield
; word addresses. The memory image is still indexed in bytes underneath.
;
        .org  0

two     .byte 1,2               ; 2 bytes -> advances 1 word
one     .byte 3                 ; 1 byte  -> still advances 1 word (rounds up)
word    .word 04321h            ; 2 bytes -> 1 word
three   .byte 4,5,6             ; 3 bytes -> advances 2 words (rounds up)
after

; The counter is a word address, so these record words not bytes.
        .word two               ; 0000
        .word one               ; 0001
        .word word              ; 0002
        .word three             ; 0003
        .word after             ; 0005 -- three bytes rounded up to two words

        .end

; A trailing comma closes a list rather than introducing one more, empty,
; argument. An empty argument anywhere else IS a zero, so exactly one is
; dropped, from the end, and only when something precedes it.
        .org  0
        .byte 1,2,3,            ; three bytes
        .byte 4,5,6             ; three bytes
        .byte 7,,8              ; three: the gap is a zero
        .byte ,1,2              ; three: a leading gap is a zero too
        .byte 9,,                ; two: 09 00
        .word $1234,            ; one word
        .text "ab",             ; two bytes
        .end

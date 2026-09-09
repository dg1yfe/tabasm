; .COMMENTCHAR (and .LOCALLABELCHAR) take the operand's SECOND character,
; unvalidated and with no closing quote required . Here the operand
; is the bare pair "Q!", so the second character '!' becomes the column-1
; comment character -- the 'Q' is ignored, and there are no quotes. A single
; change keeps pass 1 and pass 2 consistent. Verified byte-for-byte against the
; 2001 release binary: the '!' line emits nothing, so the object is 01 02.
        .org 0
        .commentchar Q!
!this whole line is a comment now and emits nothing
        .byte 1
        .byte 2
        .end

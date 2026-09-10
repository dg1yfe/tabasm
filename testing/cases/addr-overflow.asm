; Emission past the top of the image.
;
; The counter is not clamped: .ORG and the counter hold any value. A byte whose
; address exceeds $FFFF is wrapped to 16 bits and still emitted, so the run
; stays one object record rather than splitting -- five bytes from $FFFC is a
; five-byte record at FFFC. The listing shows the counter unmasked, five digits,
; which pushes the source column right by one.
;
; By default the first out-of-range address is diagnosed, once per run.
; --bug-compatibility restores the original's silence. Assembled both ways.
        .org  $FFFC
        .dw   1,2
        nop
        nop
        .end

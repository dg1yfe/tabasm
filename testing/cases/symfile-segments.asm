; The AVSIM symbol file (.AVSYM) carries the segment letter before the colon.
; This exercises all five: N C B X D, set by .NSEG/.CSEG/.BSEG/.XSEG/.DSEG.
; The D label is $12abcd to show the value is masked to 16 bits (-> abcd),
; the same as the plain file and the export file. Verified byte-for-byte
; against the 2001 release binary.
        .org 0
        .avsym
        .nseg
ns      .equ $11
        .cseg
cs      .equ $22
        .bseg
bs      .equ $33
        .xseg
xs      .equ $44
        .dseg
ds      .equ $12abcd
        .end

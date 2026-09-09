; The plain symbol file (-s or .SYM): "%-16s  %04x" per line -- name in a
; 16-wide field, two spaces, then the value as four lower-case hex digits.
; No "AS " prefix, no segment letter and no colon (that is the .AVSYM format,
; §9.6). The value is masked to 16 bits: wide below is $12abcd -> abcd.
; Verified byte-for-byte against the 2001 release binary.
        .org 0
alpha   .equ $12
beta    .equ $abcd
wide    .equ $12abcd
        nop
        .end

; The export file (.exp): one re-includable ".EQU  $hhhh" line per exported
; symbol, so it can be .INCLUDEd into another assembly. Assemble with -51; the
; 4th positional output receives it. Two things a reimplementation must match:
; the exported value is masked to 16 bits, and a name of 16+ characters is not
; truncated (the field simply grows). Verified byte-for-byte against the 2001
; release binary. Documented in the manual 9.7.
        .org 0
low     .equ $1234
wide    .equ $12abcd          ; masked to 16 bits on export -> $abcd
areallylongsymbolname .equ $beef
        .export low,wide,areallylongsymbolname
        nop
        .end

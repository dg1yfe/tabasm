; Reference to a label that is never defined.
; Exercises the "no such label" diagnostic on pass 2.
        .org  0
        mov   a,#no_such_label
        mov   dptr,#also_missing
        .end

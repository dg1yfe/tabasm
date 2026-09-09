; SJMP on the 8051 carries a single-byte signed displacement (-128..+127).
; The target here is far outside that window, exercising the range check in
; the PC-relative encoding rule (R1).
        .org  0
        sjmp  faraway
        .org  200h
faraway:
        nop
        .end

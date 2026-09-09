; An .END address above 0xFFFF.
;
; The S9 terminator prints the end address in a fixed four-digit field, so the
; operand is range-checked and masked to 16 bits rather than allowed to overflow
; it. Assemble with -g2 to reach the S-record writer.
        .org  0
        nop
        nop
        .end  12345h

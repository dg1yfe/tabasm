; A label on a .ORG line takes the address the counter is moved TO, not the
; one it is leaving. That is the only reading that makes such a label useful:
; it names the start of the block being placed.
;
; The listing shows the new address on that line too, while .BLOCK -- which
; also moves the counter -- lists the address before the move.
        .org  0
        nop
        nop
        nop
start   .org  $100
        nop
        .word start             ; $0100, not $0003
        .end

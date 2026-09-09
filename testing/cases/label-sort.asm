; The label table is ordered by a shaker (cocktail) sort keyed on the FIRST
; character, whose boundary bookkeeping stops early -- so some definition orders
; come out NOT fully sorted . Here the first characters are b,d,a,a,l
; and the result is a,b,a,d,l (ayy bee azz dee ell), not the fully sorted
; a,a,b,d,l: after one back-and-forth sweep the active window collapses with
; 'bee' still ahead of 'azz'. Verified byte-for-byte against the 2001 release
; binary; the shipped 6805 and 6800 programs exercise the same quirk.
        .org 0
bee     .equ 1
dee     .equ 2
ayy     .equ 3
azz     .equ 4
ell     .equ 5
        .end

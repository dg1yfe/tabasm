; Auxiliary-register range on the TMS320 family.
;
; Assembled twice: as "tms-arp-3225" with -3225, where it is clean, and as
; "tms-arp-3210" with -3210, where the last two LAR lines are diagnosed.
;
; TWO SEPARATE MECHANISMS produce that difference, and they are easy to
; conflate:
;
;   LARP uses the T1 rule with a validation mask taken from the table's OR
;   column -- 0001 in tasm3210.tab, 0007 in tasm3225.tab. That difference comes
;   from the TABLE, and the diagnostic is the generic
;   "Range of argument exceeded."
;
;   LAR uses the TAR rule, whose ARP width is decided by a literal string
;   comparison against the -<nn> OPTION TEXT: "3225" permits AR0-AR7, anything
;   else permits only AR0-AR1. That is independent of the table's contents, and
;   the diagnostic is the distinct "Range of ARP argument exceeded."
;
; This case pins both. It does not by itself separate them, since changing the
; selector also changes the table. The isolating experiment -- one table file
; copied under two selector names, giving different ARP widths from identical
; bytes -- is recorded in the manual
;
; A reimplementation cannot derive the ARP width from the .tab file and must
; special-case the selector. The TMS32010 has two auxiliary registers; the
; TMS320C25 has eight.
        .org  0

; Table-driven mask (T1). Clean for both parts.
        LARP  0
        LARP  1

; Table-driven mask (T1). Out of range for the 32010's 1-bit field.
        LARP  2

; Selector-driven ARP width (TAR). AR0 and AR1 suit both parts.
        LAR   0,10
        LAR   1,11

; Selector-driven ARP width (TAR). AR5 and AR7 need the C25.
        LAR   5,12
        LAR   7,13

        .end

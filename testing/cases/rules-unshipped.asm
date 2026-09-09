; Encoding rules that no shipped table selects (spec 07-encoding-rules.md 7.25).
; Assemble with -51: tasm51 declares .NOARGSHIFT and is LSFIRST, so an injected
; row behaves exactly as it would in its intended table. Each rule is added with
; .ADDINSTR and then exercised; the expected emission is in the comment.
;
; Verified byte-for-byte against the 2001 release binary (tasm.exe under wine).
; Documented in the notes and the manual 7.25.
        .org 0

        .addinstr TCN  *,*    90 2 CN 1
        .addinstr TC5  *,*    91 2 C5 1
        .addinstr T3A  *,*,*  92 4 3A 1
        .addinstr TR3  *      A0 1 R3 1
        .addinstr TZW  *,*    C4 3 ZW 1
        .addinstr TZD  *,*    C6 2 ZD 1
        .addinstr TZL  *,*    C8 2 ZL 1
        .addinstr TSZ  *      C3 3 SZ 1
        .addinstr TSB  *,*    10 2 SB 1
        .addinstr TSB3 *,*,*  20 3 SB 1
        .addinstr TT2  *,*    30 3 T2 1
        .addinstr TT2A *      40 1 T2 1
        .addinstr TT3  *,*    50 3 T3 1
        .addinstr TT4  *,*    0000 2 T4 1
        .addinstr TT6  *,*    60 1 T6 1 0 F0

        TCN  3,5            ; CN  -> 90 53
        TC5  3,5            ; C5  -> 91 35
        T3A  11h,22h,33h    ; 3A  -> 92 11 22 33
        TR3  $+3            ; R3  -> A2         delta 2, A0|2
        TZW  0E1h,0E2h      ; ZW  -> C2 12      both working regs: opcode C4->C2, one byte
        TZW  11h,22h        ; ZW  -> C4 22 11   not working regs: byte-swapped pair
        TZD  3,$+5          ; ZD  -> F6 03      opcode C6|(3<<4), delta 3
        TZL  55h,2          ; ZL  -> E8 55      opcode C8|(2<<4), immed 0x55
        TSZ  44h            ; SZ  -> B3 44      zero page: C3->B3, one byte
        TSZ  1234h          ; SZ  -> C3 12 34   not zero page: byte-swapped
        TSB  44h,3          ; SB  -> 16 44      opcode 10|(3<<1), addr 0x44
        TSB3 44h,3,$+4      ; SB  -> 26 44 01   opcode 20|(3<<1), delta 1
        TT2  1234h,7        ; T2  -> 37 12 34   opcode 30|7, const byte-swapped
        TT2A 5              ; T2  -> 45         opcode 40|5, one operand
        TT3  1,5678h        ; T3  -> 51 56 78   opcode 50|1, const byte-swapped
        TT4  1,2            ; T4  -> 81 00      opcode 1|(2<<6)=0x81, LSFIRST
        TT6  3,50h          ; T6  -> 73         opcode 60|3|50

        .end

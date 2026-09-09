; Macro parameter references (spec 04-directives.md 4.7). A parameterised macro
; body refers to its parameters two ways, and both work:
;   - by the parameter's NAME, which .DEFINE rewrites to the internal token ?n;
;   - by that ?n token directly (n = the 0-based parameter index).
; At each call the ?n are spliced with the call arguments. Substitution is by
; WHOLE IDENTIFIER: a parameter name is not replaced inside a longer token, so
; a parameter A leaves the A inside 0Ah alone. Verified byte-for-byte against
; the 2001 release binary. Documented in the notes.
        .org 0
#define BYNAME(x,y)  .byte x,y
#define BYINDEX(a,b) .byte ?0,?1
#define DUP(v)       .byte v,v,v
#define WHOLEID(A)   .byte A,0Ah
        BYNAME(0AAh,0BBh)   ; by name        -> AA BB
        BYINDEX(0CCh,0DDh)  ; by ?n index    -> CC DD
        DUP(7)              ; repeated use   -> 07 07 07
        WHOLEID(3)          ; A in 0Ah kept  -> 03 0A
        .end

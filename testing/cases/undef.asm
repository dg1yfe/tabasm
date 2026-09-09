; The .UNDEF directive.
; Assemble with -51.
;
; .UNDEF was listed in the directive table but implemented in neither pass, so
; it parsed cleanly and did nothing at all. It is honoured now; the same source
; under --compatibility (case "undef-compat") restores the old silence, and the
; two golden listings differ only in which branches were taken.
;
; Note the '#' prefix throughout. Macro expansion runs before parsing, so in
; ".undef FOO" the operand FOO would be replaced by its own definition before
; the directive ever saw it. A leading '#' suppresses expansion for the line.
; This is pre-existing behaviour, not a restriction of the fix: ".ifdef FOO"
; is affected in exactly the same way, which is why the shipped headers all
; use the '#' forms.
;
        .org  0

#define FOO 1
#define BAR 2

; Both defined at this point.
#ifdef FOO
        .byte 011h              ; emitted: FOO is defined
#endif
#ifdef BAR
        .byte 022h              ; emitted: BAR is defined
#endif

; Remove one of them.
#undef FOO

#ifdef FOO
        .byte 0AAh              ; default: NOT emitted.  --compatibility: emitted
#else
        .byte 0BBh              ; default: emitted.      --compatibility: not
#endif

; The other is untouched, which checks that removal compacts the table without
; disturbing its neighbours.
#ifdef BAR
        .byte 033h              ; emitted in both modes
#endif

; Undefining a name that is not a macro is not an error.
#undef NEVER_DEFINED
        .byte 044h              ; emitted in both modes

        .end

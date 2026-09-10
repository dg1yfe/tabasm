; A conditional nested inside the skipped branch of another must stay inert.
; Its own #ELSE must not re-open assembly that the enclosing level excluded:
; a level opened while skipping counts as already taken, so no branch of it
; can ever fire.
;
; Assembled three ways -- -dOUTER, -dINNER and neither -- which take the
; outer, middle and innermost branch in turn. The middle and innermost cases
; pass even without that rule; only the outer one catches the defect.
#IFDEF OUTER
#DEFINE VAL 1
#ELSE
#IFDEF INNER
#DEFINE VAL 2
#ELSE
#DEFINE VAL 3
#ENDIF
#ENDIF

; A second level of nesting, and an #IFNDEF, in a branch that is skipped.
#IFDEF OUTER
#DEFINE DEPTH 0
#ELSE
#IFNDEF INNER
#IFDEF NOPE
#DEFINE DEPTH 1
#ELSE
#DEFINE DEPTH 2
#ENDIF
#ELSE
#DEFINE DEPTH 3
#ENDIF
#ENDIF

        .org  $100
        ldaa  #VAL
        ldab  #DEPTH
        .end

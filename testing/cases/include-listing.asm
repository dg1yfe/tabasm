; An INCLUDE is listed before the file it pulls in, so the listing reads in
; the order the source is read, and the included lines carry a depth marker in
; column 11. The marker is 1-based: the file named on the command line is
; depth 1 and unmarked, its includes are depth 2 and marked `+`.
        .org  0
        nop
#include "include-listing-inc.asm"
        nop
        .end

; CRLF line endings. Assemble with -51.
;
; THIS FILE HAS CRLF LINE ENDINGS ON PURPOSE. If your editor or git normalises
; it to LF, the case stops testing anything. A .gitattributes rule marks it
; -text to prevent that.
;
; DOS-era assembly source has CRLF endings -- the original TASM distribution's
; own examples included -- and this assembler's natural corpus is exactly that.
; The Windows build never had to think about it, because its C runtime strips
; CR when reading a text file. On a Unix host that translation does not happen,
; so the CR survived into the line buffer and was absorbed into whichever token
; ended the line: ".END" became ".END\r" and matched no directive, "0" became
; "0\r" and was not a number.
;
; The CR is now stripped at the read, reproducing what the Windows runtime did.
;
; Every construct below has its last token at end of line, which is where the
; CR lands.
;
; A source file written on Windows must assemble to the same bytes as the
; same file written on Unix. That is what this case pins.
        .org 0

start:  nop
        mov  a,#5
        .byte 1,2,3
        .word 1234h
        .text "crlf"
value   .equ 42
        mov  a,#value
        sjmp start
        .end

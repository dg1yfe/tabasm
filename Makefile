# Build and install tabasm.
#
#   make                       build
#   sudo make install          install to /usr/local
#   make install PREFIX=/usr   install elsewhere
#   sudo make uninstall
#
# DESTDIR stages the install under another root without touching the real
# filesystem, which is what a package build wants:
#
#   make install DESTDIR=/tmp/stage PREFIX=/usr
#
# src/Makefile is a different thing: a convenience wrapper giving a binary at a
# fixed path, a SANITIZE=1 build that traps arithmetic overflow, and a checksec
# target. It installs nothing.

PREFIX  ?= /usr/local
DESTDIR ?=

BINDIR   = $(DESTDIR)$(PREFIX)/bin
SHAREDIR = $(DESTDIR)$(PREFIX)/share
TABLEDIR = $(SHAREDIR)/tabasm/tables
MANDIR   = $(SHAREDIR)/man/man1
DOCDIR   = $(SHAREDIR)/doc/tabasm

INSTALL ?= install

.PHONY: all build install uninstall test clean

all: build

build:
	cargo build --release --locked

test:
	cargo test --release --locked

# The tables are not optional extras: without them the assembler knows no
# instruction set. They go to $(PREFIX)/share/tabasm/tables, which is where a
# binary in $(PREFIX)/bin looks for them, so an install needs no environment
# variable set afterwards.
install: build
	$(INSTALL) -d $(BINDIR) $(TABLEDIR) $(MANDIR) $(DOCDIR)
	$(INSTALL) -m 755 target/release/tabasm $(BINDIR)/tabasm
	$(INSTALL) -m 644 tables/*.tab2 $(TABLEDIR)/
	$(INSTALL) -m 644 doc/tabasm.1 $(MANDIR)/tabasm.1
	$(INSTALL) -m 644 README.md NOTICE LICENSE $(DOCDIR)/
	$(INSTALL) -m 644 doc/table-format.md doc/table-format-legacy.md $(DOCDIR)/
	@echo "installed to $(DESTDIR)$(PREFIX)"

uninstall:
	rm -f $(BINDIR)/tabasm $(MANDIR)/tabasm.1
	rm -rf $(SHAREDIR)/tabasm $(DOCDIR)

clean:
	cargo clean

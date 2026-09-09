# Build wrapper for the test harness.
#
# Some harnesses expect an executable at src/tabasm built
# by `make -C src`. The cargo project itself lives at the repository root in
# the idiomatic layout (Cargo.toml + src/*.rs); this Makefile only drives it
# and drops the binary where the harness looks.
#
#   make -C src              release build
#   make -C src SANITIZE=1   debug build, overflow and debug assertions on
#   make -C src checksec     report the hardening posture of the built binary
#   make -C src clean        remove the binary and the cargo target directory

ROOT     := $(CURDIR)/..
MANIFEST := $(ROOT)/Cargo.toml
BIN      := tabasm

ifdef SANITIZE
# Rust gives memory safety without a sanitizer; what a sanitizer build adds
# here is arithmetic overflow trapping and debug assertions, which is where
# the interesting failures in this program live. Both are off in release.
CARGO_PROFILE :=
PROFILE_DIR   := debug
export RUSTFLAGS := -C debug-assertions=on -C overflow-checks=on
else
CARGO_PROFILE := --release
PROFILE_DIR   := release
endif

.PHONY: all clean checksec

all: $(BIN)

$(BIN): FORCE
	cargo build --manifest-path $(MANIFEST) $(CARGO_PROFILE)
	cp -f $(ROOT)/target/$(PROFILE_DIR)/$(BIN) $@

FORCE:

clean:
	cargo clean --manifest-path $(MANIFEST) 2>/dev/null || true
	rm -f $(BIN)

checksec: $(BIN)
	@echo "binary:    $(CURDIR)/$(BIN)"
	@echo "profile:   $(PROFILE_DIR)"
	@echo "language:  Rust (no unsafe blocks)"
	@printf 'unsafe:    '; \
	  if grep -rn --include='*.rs' -w unsafe $(ROOT)/src >/dev/null 2>&1; \
	  then grep -rn --include='*.rs' -w unsafe $(ROOT)/src; \
	  else echo 'none'; fi
	@printf 'overflow:  '; \
	  if [ "$(PROFILE_DIR)" = debug ]; then echo 'trapping (debug-assertions, overflow-checks)'; \
	  else echo 'wrapping (release); build with SANITIZE=1 to trap'; fi
	@command -v otool >/dev/null 2>&1 && { echo 'linkage:'; otool -L $(BIN) | sed 1d; } || true

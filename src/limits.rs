//! Capacities and exit codes.
//!
//! The original derived these from fixed-size C arrays. Growable storage means
//! we need not allocate that way, but the limits are still enforced, and at
//! the same thresholds: source that silently assembles in one build and fails
//! in another is worse than either outcome alone. So they are policy here
//! rather than a property of the storage.

// --- exit codes (1.8) -------------------------------------------------------
pub const EXIT_OK: i32 = 0; // assembly completed, no errors
pub const EXIT_ERRORS: i32 = 1; // completed with one or more errors
pub const EXIT_NOMEM: i32 = 2; // memory allocation failure
pub const EXIT_FILE: i32 = 3; // file access failure
pub const EXIT_FATAL: i32 = 4; // fatal internal error

// --- capacities (10.1) ------------------------------------------------------
pub const LINE_SIZE: usize = 512; // read buffer; 511 bytes of a line are seen
pub const MAX_SYMBOLS: usize = 15_000; // non-fatal: label dropped
pub const MAX_LABEL: usize = 31; // usable characters, longer is truncated
pub const MAX_TABLE_ROWS: usize = 1_200; // fatal, exit 4
pub const MAX_REGSETS: usize = 32; // fatal, exit 4
pub const MAX_ARGS: usize = 128; // args per instruction or directive
pub const MAX_MACROS: usize = 1_000; // non-fatal: macro dropped
pub const MAX_MACRO_PARAMS: usize = 10;
pub const MAX_MACRO_PARAM_LEN: usize = 16;
pub const MAX_CONDITIONALS: usize = 32; // 31 usable: depth checked before use
pub const MAX_INCLUDE_DEPTH: usize = 16;
pub const MAX_BASENAME: usize = 79;
pub const MAX_FILE_ARGS: usize = 5; // src, obj, lst, exp, sym

// --- address space (2.3) ----------------------------------------------------
pub const IMAGE_SIZE: usize = 0x1_0000;

/// Undefined symbols evaluate to this (2.1, 3.8): deliberately outside the
/// 16-bit address space so the shortening rules decline to shorten in pass 1.
/// Its low 16 bits are zero, which is why those rules must test the full value
/// and not just `aval`.
pub const UNDEFINED: i32 = 0x2_0000;

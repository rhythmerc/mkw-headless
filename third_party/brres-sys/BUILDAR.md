# brres-sys 0.1.12, patched

riidefi's `brres-sys` (MIT; https://github.com/riidefi/RiiStudio), as published on crates.io, with one change: its
two C-string parameters in `src/ffi.rs` are `*const c_char` (they were `*const i8`, which doesn't build where `char` is
unsigned: arm64, the headset). Patched in by `[patch.crates-io]` until a release has the fix.

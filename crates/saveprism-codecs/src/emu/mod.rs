//! Emulator saves: Snes9x snapshots, RetroArch save states and RetroArch `.srm` battery saves.
//!
//! The Snes9x snapshot layout comes from the public `docs/snapshots.txt` shipped with the
//! emulator ("snapshot file format", version 0006; documentation only, no code was read). The
//! RetroArch `RASTATE` layout (7-byte magic, version byte 1, then blocks of a 4-byte tag, a
//! little-endian `u32` payload length and the payload padded to 8 bytes) and the `.srm` battery
//! format are the public facts stated in this project's brief; the state is shown as data and
//! never executed.
//!
//! * `format.snes9x-snapshot`: `#!s9xsnp:NNNN\n` followed by `NAM:000123:...` blocks. The `SRA`
//!   (save RAM) block is editable raw bytes; all other blocks are read-only. A gzip-wrapped file is
//!   peeled by `compression.gzip` first.
//! * `format.retroarch-state`: `RASTATE` blocks are shown read-only (the `MEM ` block is the
//!   core's own serialization, not raw SRAM). A file-level RZIP container opens read-only with a
//!   hint to turn off Save State File Compression in RetroArch.
//! * `format.emu-sram`: a plain `.srm` battery save, raw bytes. Its size is fixed; the bytes are
//!   editable and written back exactly when the length does not change.

mod retroarch;
mod snes9x;
mod sram;

#[cfg(test)]
mod tests;

use saveprism_core::Registry;

/// Registers the three emulator codecs.
pub fn register(reg: &mut Registry) {
    reg.add_codec(std::sync::Arc::new(snes9x::Snes9xSnapshotCodec));
    reg.add_codec(std::sync::Arc::new(retroarch::RetroArchStateCodec));
    reg.add_codec(std::sync::Arc::new(sram::EmulatorSramCodec));
}

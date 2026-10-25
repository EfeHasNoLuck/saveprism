//! Terraria player (`.plr`) and world (`.wld`) saves.
//!
//! Layout read from public documentation and the MS-PL-licensed
//! [TEdit/Terraria-Map-Editor](https://github.com/TEdit/Terraria-Map-Editor) sources
//! (`src/TEdit.Terraria/Player/PlayerFile.cs`, `World.FileV2.cs`, `WorldConfiguration.cs`,
//! `FileType.cs`); only behavior was read, no code was copied. The save folder and file
//! extensions are from the Terraria wiki (`Character` and `Server` pages, checked 2026-10-25):
//! `%USERPROFILE%\Documents\My Games\Terraria\Players\*.plr` and `...\Worlds\*.wld`.
//!
//! # Player
//!
//! A `.plr` file is AES-CBC encrypted with the UTF-16LE bytes of an 8 character string constant
//! from the game's own assembly (`Terraria.exe`) used as both key and IV, with no padding beyond
//! what the writer adds to reach a whole block. This crate never holds that constant: the
//! `crypto.terraria-plr` layer takes it from the recipe hint `password`, and the desktop key
//! finder reads the `#US` heap of the player's own `Terraria.exe` (AGENTS.md rule 7).
//!
//! After decryption, the player document starts with a little-endian `i32` version. Version 135
//! and later add a Re-Logic header: `u64` (low 56 bits `0x006369676f6c6572` = the bytes
//! `relogic\0`, top byte 3 = player), a `u32` revision and a `u64` flag word whose bit 0 is the
//! favorite flag. Then come the name (.NET `BinaryReader.ReadString`: 7-bit length + UTF-8),
//! difficulty, play time (ticks), appearance, the four life/mana integers, colors, armor, dyes
//! and the inventory. This codec exposes the documented header fields and inventory slots as
//! editable values; every byte after the inventory (buffs, banks, journey powers, ...) is kept
//! as read-only `tail` bytes and written back untouched.
//!
//! # World
//!
//! A `.wld` file is not encrypted. It starts with a `u32` version; version 140 and later add the
//! 7 byte `relogic` (or `xindong`) header and a `u8` file type 2. The next `i16` is the section
//! count, followed by one `i32` end pointer per section, then the bit-packed
//! "tile frame important" array (`i16` bit count + the bytes). Section 1 holds the world header
//! values (name, seed, dimensions, ids, game mode, special seeds, times and flags); sections 2
//! to 10 hold tiles, chests, signs, NPCs, tile entities, pressure plates, town manager, bestiary
//! and creative powers. Those are exposed as read-only `sections` bytes and kept byte-identical.
//! The section pointers are rewritten when an edited header changes its encoded length, so the
//! game still finds every section; no checksum exists in the format (checked against TEdit).
//!
//! # Exact writing
//!
//! Every field is written in file order with its canonical encoding. Untouched files therefore
//! reproduce their original bytes. The player's name and the world strings use .NET's canonical
//! 7-bit length prefix. The player `tail` and the world `sections` keep their original bytes.

pub mod aes;
pub mod player;
pub mod world;

#[cfg(test)]
mod tests;

use saveprism_core::{ErrorCode, Registry};

/// A `.plr` file's plaintext is shorter than its header or one of its fields does not fit.
pub const TERRARIA_PARSE: ErrorCode = ErrorCode::new("SP-TERRARIA-001");
/// A file uses a layout or version this codec does not place.
pub const TERRARIA_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-TERRARIA-002");
/// The document cannot be written as a Terraria save (number out of range, bad name, no header).
pub const TERRARIA_WRITE: ErrorCode = ErrorCode::new("SP-TERRARIA-003");
/// The encrypted `.plr` is malformed (empty, not whole blocks).
pub const TERRARIA_BAD_DATA: ErrorCode = ErrorCode::new("SP-TERRARIA-004");
/// The key did not decrypt the file (wrong password, or not a Terraria player).
pub const TERRARIA_WRONG_PASSWORD: ErrorCode = ErrorCode::new("SP-TERRARIA-005");
/// The encrypted `.plr` needs its key and none was given.
pub const TERRARIA_PASSWORD_REQUIRED: ErrorCode = ErrorCode::new("SP-TERRARIA-006");

/// The 7 low bytes of the 1.3+ Re-Logic header (`relogic` in little-endian order).
pub const RELOGIC_MAGIC: u64 = 0x0063_6967_6f6c_6572;
/// File type byte of a player in the Re-Logic header.
pub const FILE_TYPE_PLAYER: u8 = 3;
/// File type byte of a world in the Re-Logic header.
pub const FILE_TYPE_WORLD: u8 = 2;

/// Registers the Terraria codecs and the player encryption layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(std::sync::Arc::new(aes::TerrariaPlayerLayer));
    reg.add_codec(std::sync::Arc::new(player::TerrariaPlayerCodec));
    reg.add_codec(std::sync::Arc::new(world::TerrariaWorldCodec));
}

/// True when a Re-Logic header carries `magic` (low 56 bits) and `file_type` (top byte).
pub(crate) fn relogic_header(header: u64, file_type: u8) -> bool {
    header & 0x00FF_FFFF_FFFF_FFFF == RELOGIC_MAGIC && (header >> 56) as u8 == file_type
}

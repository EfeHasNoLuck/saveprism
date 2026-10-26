//! `format.pixel-crushers`: Pixel Crushers Save System (and Dialogue System) saves.
//!
//! The Pixel Crushers Common Library saves a `SavedGameData` object with the assigned
//! `DataSerializer` (JSON by default) and stores it in `save<N>.dat` files or PlayerPrefs. The
//! object has a version, the scene name and a list of save records; every record has a key, a
//! scene index and the saver's data as a string. The Dialogue System's saver puts its
//! `PersistentDataManager.GetSaveData()` result there, which is human-readable Lua assignment
//! text (`Variable["Gold"]=10; ...`).
//!
//! This codec decodes the outer JSON and inlines each record's data string as its own child tree:
//! JSON for savers that serialize JSON, and the data-only Lua reader in [`lua`] for Dialogue
//! System data. Strings inside strings are written back with the original text when unchanged.
//! The Lua text is never executed; it is parsed as literals only.
//!
//! Encrypted `DiskSavedGameDataStorer` files are not supported: the public reference names an
//! `encryptionPassword` field but no algorithm, so guessing would be unsafe.
//!
//! Sources (behaviour only, no code copied): the public Dialogue System manual page
//! [`save_system.html`](https://pixelcrushers.com/dialogue_system/manual2x/html/save_system.html),
//! the class references for [`SavedGameData`](https://pixelcrushers.com/dialogue_system/manual2x/html/class_pixel_crushers_1_1_saved_game_data.html)
//! (version, scene name, records), [`JsonDataSerializer`](https://pixelcrushers.com/dialogue_system/manual2x/html/class_pixel_crushers_1_1_json_data_serializer.html)
//! (JsonUtility), [`DiskSavedGameDataStorer`](https://pixelcrushers.com/dialogue_system/manual2x/html/class_pixel_crushers_1_1_disk_saved_game_data_storer.html)
//! (files, encryption fields) and [`PersistentDataManager`](https://pixelcrushers.com/dialogue_system/manual2x/html/class_pixel_crushers_1_1_dialogue_system_1_1_persistent_data_manager.html)
//! (human-readable Lua save data).

mod lua;
mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The input is not the expected outer JSON (no save records).
pub const PC_PARSE: ErrorCode = ErrorCode::new("SP-PC-001");
/// A record's data string could not be inlined.
pub const PC_RECORD: ErrorCode = ErrorCode::new("SP-PC-002");
/// The document cannot be written back.
pub const PC_WRITE: ErrorCode = ErrorCode::new("SP-PC-003");
/// The save is encrypted and the public docs name no algorithm or key derivation.
pub const PC_ENCRYPTED: ErrorCode = ErrorCode::new("SP-PC-004");

/// Hint of a nested JSON record.
pub(crate) const JSON_HINT: &str = "PC:json";
/// Hint of a nested Dialogue System Lua record.
pub(crate) const LUA_HINT: &str = "PC:lua";
/// Keys that can hold the record list in different SavedGameData serializations.
pub(crate) const RECORD_KEYS: [&str; 3] = ["saveRecords", "records", "saveRecordList"];

/// Registers the Pixel Crushers codec as `format.pixel-crushers`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(PixelCrushersCodec));
}

/// The `format.pixel-crushers` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct PixelCrushersCodec;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

impl Codec for PixelCrushersCodec {
    fn id(&self) -> &'static str {
        "format.pixel-crushers"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let body = input.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(input);
        let first = body
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'));
        if first.and_then(|index| body.get(index)) != Some(&b'{') {
            return Confidence::NONE;
        }
        if !contains(body, b"\"data\"") {
            return Confidence::NONE;
        }
        // `saveRecords` is specific enough to claim the file; the other spellings only raise the
        // score when the surrounding SavedGameData fields are there too, so a plain JSON save
        // that happens to have a `records` key is not taken from `format.json`.
        let named = contains(body, b"\"saveRecords\"")
            || contains(body, b"\"saveRecordList\"")
            || (contains(body, b"\"records\"")
                && (contains(body, b"\"sceneName\"") || contains(body, b"\"version\"")));
        if named {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let looks_json =
            std::str::from_utf8(input).is_ok_and(|text| text.trim_start().starts_with('{'));
        if !looks_json {
            return Err(Error::new(
                PC_PARSE,
                "a Pixel Crushers save is JSON; an encrypted save is not supported because the \
                 public documentation names no encryption algorithm",
            ));
        }
        read::decode(input, ctx)
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

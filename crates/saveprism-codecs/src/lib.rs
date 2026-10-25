//! SavePrism codecs.
//!
//! Layers (`layers`) and formats (`json`, `es3`, `gvas`, `marshal`, `lcf`, ...) implement the `Layer` and `Codec` traits of
//! `saveprism-core`. [`register_all`] adds all of them to a [`Registry`].

use saveprism_core::Registry;

pub mod bson;
pub mod cbor;
pub mod csv_text;
pub mod es3;
pub mod gamemaker;
pub mod gen3;
pub mod godot;
pub mod gvas;
pub mod ini;
pub mod javaser;
pub mod json;
pub mod layers;
pub mod lcf;
pub mod marshal;
pub mod msgpack;
pub mod naninovel;
pub mod nbt;
pub mod nrbf;
pub mod odin;
pub mod palworld;
pub mod pickle;
pub mod plist;
pub mod properties;
pub mod qsp;
pub mod raw;
pub mod reg;
pub mod renpy;
pub mod rpgmaker;
pub mod sol;
pub mod sqlite;
pub mod terraria;
mod text_common;
mod text_encoding;
pub mod tjs2;
pub mod toml_text;
pub mod witcher3;
pub mod wolf;
pub mod xml;
pub mod yaml;
mod zip_common;

/// Turns an encoding label (a name, a WHATWG label or an EasyRPG `RPG_RT.ini` code page) into the
/// canonical name this crate stores in a step. Returns `None` for an encoding it cannot use.
pub fn resolve_text_encoding(label: &str) -> Option<&'static str> {
    text_encoding::TextEncoding::from_name(label).map(text_encoding::TextEncoding::name)
}

/// Registers every layer and codec of this crate into `reg`.
pub fn register_all(reg: &mut Registry) {
    layers::register(reg);
    raw::register(reg);
    msgpack::register(reg);
    cbor::register(reg);
    bson::register(reg);
    nbt::register(reg);
    json::register(reg);
    rpgmaker::register(reg);
    es3::register(reg);
    gamemaker::register(reg);
    csv_text::register(reg);
    gen3::register(reg);
    gvas::register(reg);
    ini::register(reg);
    palworld::register(reg);
    marshal::register(reg);
    naninovel::register(reg);
    javaser::register(reg);
    lcf::register(reg);
    pickle::register(reg);
    plist::register(reg);
    properties::register(reg);
    qsp::register(reg);
    reg::register(reg);
    renpy::register(reg);
    terraria::register(reg);
    sol::register(reg);
    nrbf::register(reg);
    odin::register(reg);
    godot::register(reg);
    sqlite::register(reg);
    tjs2::register(reg);
    toml_text::register(reg);
    wolf::register(reg);
    witcher3::register(reg);
    yaml::register(reg);
    xml::register(reg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_all_works_on_an_empty_registry() {
        let mut reg = Registry::default();
        register_all(&mut reg);
        // Registering twice must be harmless (same ids replace each other).
        register_all(&mut reg);
    }
}

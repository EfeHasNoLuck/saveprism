//! SavePrism codecs.
//!
//! Layers (`layers`) and formats (`json`, more later) implement the `Layer` and `Codec` traits of
//! `saveprism-core`. [`register_all`] adds all of them to a [`Registry`].

use saveprism_core::Registry;

pub mod es3;
pub mod gvas;
pub mod json;
pub mod layers;
pub mod rpgmaker;

/// Registers every layer and codec of this crate into `reg`.
pub fn register_all(reg: &mut Registry) {
    layers::register(reg);
    json::register(reg);
    rpgmaker::register(reg);
    es3::register(reg);
    gvas::register(reg);
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

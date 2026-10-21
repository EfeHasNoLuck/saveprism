//! The registry maps step identifiers to their [`Layer`] and [`Codec`] implementations.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::codec::{Codec, Layer};

/// Lookup table for layers and codecs, keyed by their identifier (`"compression.gzip"`).
/// `saveprism-codecs` fills it with `register_all`.
#[derive(Default)]
pub struct Registry {
    layers: BTreeMap<&'static str, Arc<dyn Layer>>,
    codecs: BTreeMap<&'static str, Arc<dyn Codec>>,
}

impl Registry {
    /// Registers a layer; a layer with the same identifier is replaced.
    pub fn add_layer(&mut self, layer: Arc<dyn Layer>) {
        self.layers.insert(layer.id(), layer);
    }

    /// Registers a codec; a codec with the same identifier is replaced.
    pub fn add_codec(&mut self, codec: Arc<dyn Codec>) {
        self.codecs.insert(codec.id(), codec);
    }

    /// The layer with this identifier.
    pub fn layer(&self, id: &str) -> Option<&dyn Layer> {
        self.layers.get(id).map(Arc::as_ref)
    }

    /// The codec with this identifier.
    pub fn codec(&self, id: &str) -> Option<&dyn Codec> {
        self.codecs.get(id).map(Arc::as_ref)
    }

    /// All layers, sorted by identifier.
    pub fn layers(&self) -> impl Iterator<Item = &Arc<dyn Layer>> {
        self.layers.values()
    }

    /// All codecs, sorted by identifier.
    pub fn codecs(&self) -> impl Iterator<Item = &Arc<dyn Codec>> {
        self.codecs.values()
    }

    /// True when a layer with this identifier is registered.
    pub fn is_layer(&self, id: &str) -> bool {
        self.layers.contains_key(id)
    }

    /// True when a codec with this identifier is registered.
    pub fn is_codec(&self, id: &str) -> bool {
        self.codecs.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Confidence, DecodeCtx, Decoded, SniffCtx};
    use crate::doc::SaveDoc;
    use crate::error::Result;
    use crate::params::Params;

    struct FakeLayer(&'static str);

    impl Layer for FakeLayer {
        fn id(&self) -> &'static str {
            self.0
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.to_vec(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.to_vec())
        }
    }

    struct FakeCodec(&'static str);

    impl Codec for FakeCodec {
        fn id(&self) -> &'static str {
            self.0
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            Ok(Decoded {
                doc: SaveDoc::new(),
                params: Params::new(),
            })
        }
        fn encode(&self, _doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn lookup_and_kind_checks() {
        let mut reg = Registry::default();
        reg.add_layer(Arc::new(FakeLayer("b.layer")));
        reg.add_codec(Arc::new(FakeCodec("a.codec")));
        assert!(reg.is_layer("b.layer"));
        assert!(!reg.is_codec("b.layer"));
        assert!(reg.is_codec("a.codec"));
        assert!(!reg.is_layer("a.codec"));
        assert_eq!(reg.layer("b.layer").map(|l| l.id()), Some("b.layer"));
        assert_eq!(reg.codec("a.codec").map(|c| c.id()), Some("a.codec"));
        assert!(reg.layer("missing").is_none());
        assert!(reg.codec("missing").is_none());
    }

    #[test]
    fn iteration_is_sorted_by_id() {
        let mut reg = Registry::default();
        reg.add_layer(Arc::new(FakeLayer("z.layer")));
        reg.add_layer(Arc::new(FakeLayer("a.layer")));
        reg.add_layer(Arc::new(FakeLayer("m.layer")));
        reg.add_codec(Arc::new(FakeCodec("y.codec")));
        reg.add_codec(Arc::new(FakeCodec("b.codec")));
        let layer_ids: Vec<&str> = reg.layers().map(|l| l.id()).collect();
        assert_eq!(layer_ids, ["a.layer", "m.layer", "z.layer"]);
        let codec_ids: Vec<&str> = reg.codecs().map(|c| c.id()).collect();
        assert_eq!(codec_ids, ["b.codec", "y.codec"]);
    }

    #[test]
    fn adding_the_same_id_replaces_the_old_entry() {
        let mut reg = Registry::default();
        reg.add_layer(Arc::new(FakeLayer("x.layer")));
        reg.add_layer(Arc::new(FakeLayer("x.layer")));
        assert_eq!(reg.layers().count(), 1);
    }
}

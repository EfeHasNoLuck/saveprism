# D7-duzeltme-2

- tests/es3_aes.rs: `Limits::default()` bound to `let limits` before `DecodeCtx::new(&limits)` (E0716). No other instance of the pattern in the file.
- tests/palworld.rs: removed unused `Codec` import; `zlib` and `make_plz` now return `Result<_, Box<dyn Error>>` and use `?` (incl. `u32::try_from`); `#[test]` callers `.unwrap()` the result.
- cli/src/lib.rs: removed unused `Layer` import.
- Verify: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Not compiled by me. Possible leftover: rustfmt may want to rewrap the changed `use` line in cli/src/lib.rs.

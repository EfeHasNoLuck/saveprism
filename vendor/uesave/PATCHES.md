# uesave 0.7.1, patched for SavePrism

Source: the crates.io package `uesave` 0.7.1 (https://github.com/trumank/uesave, MIT, text in `COPYING`).
Only `src/` (without `tests.rs`), `README.md` and the manifest (without the `[[example]]` entries) are copied.
The workspace `Cargo.toml` points `uesave` here with `[patch.crates-io]`.

## Changes (search for "SavePrism patch")
1. `SeekReader::ensure_buffered` reads in 64 KiB chunks instead of allocating the whole requested size first.
2. `read_string` (positive length) grows the string buffer with the bytes actually read instead of allocating
   `len` zeroed bytes first. A short input still fails with `UnexpectedEof` ("failed to fill whole buffer").

## Why
SavePrism's GVAS hint search tries wrong struct types on purpose. A wrong guess makes uesave read a garbage
length of up to 2 GiB and allocate zeroed buffers of that size: 5.3 GB in total for a 379-byte test save.
Windows and Linux hand back fresh zero pages almost for free; macOS reuses cached large blocks and zeroes them,
so the search ran out of its 3 s budget there and some Unreal saves did not open. Parsing of valid files is
unchanged. Drop this copy when an upstream release reads lengths this way.

# D7 kanıt (2026-10-22)

```

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

cargo test --workspace: 301 passed, 0 failed, 1 ignored (bench_gvas)
cargo deny check: advisories ok, bans ok, licenses ok, sources ok
bench_gvas --release: 52.00 MB, uesave read 254 ms, decode 391 ms, encode 103 ms, byte identical
keys try settings.es3 (3 candidates): found: synthetic-test-key (line 3), exit 0; wrong list: SP-CLI-002, exit 1
CLI decode -> encode identical: synthetic/es3/progress.es3, drg-save-test.sav
CLI edit: progress.es3 coins 250 -> 251, only that byte range changed, inspect exact
```

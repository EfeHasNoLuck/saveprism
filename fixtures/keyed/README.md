# Keyed fixtures

Synthetic files that need a key to open. The golden test runner does not scan this folder (it has no keys);
each format's own test opens these files with the key written here. Keys here are invented for tests and
belong to no game.

| Folder | Key | Made with | How |
|---|---|---|---|
| `es3-aes/` | password `synthetic-test-key` | .NET Framework `Rfc2898DeriveBytes` + `Aes` (Windows PowerShell 5.1), [`../tools/make_es3_aes.ps1`](../tools/make_es3_aes.ps1) | Easy Save 3 layout: `iv[16] ‖ AES-128-CBC(PBKDF2-HMAC-SHA1(password, iv, 100, 16), iv, PKCS7)`. `progress-gzip.es3` is GZip-compressed before encryption. Fixed IVs (`00..0f`, `10..1f`) so the files can be rebuilt. Both files were decrypted again with Python `cryptography` + `hashlib.pbkdf2_hmac` as an independent check (2026-10-22). Data is invented. |

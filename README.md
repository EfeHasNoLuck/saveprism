# SavePrism

A free, local, MIT-licensed desktop app for editing game save files. Open a save from RPG Maker, Unity, Unreal Engine and more,
change values like gold, stats and items, and save it back **byte-for-byte in its original format**. Your files never leave your computer.

**Status:** pre-release. The core and the command-line tool work today; the Windows desktop app (v0.1) is being built.

---

## Why

1. **Online editors have limits:** upload caps, waiting times, paywalls, and your file goes to someone else's server.
2. **Open-source tools are scattered:** one CLI per engine, and the best desktop editors cover only a few engines.
3. **Nothing round-trips safely:** SavePrism decodes a save, re-encodes it unchanged and checks that the bytes match before it lets you
   edit. Every write makes a backup first and is verified afterwards.

## Supported formats

| Engine | Files | Notes |
|---|---|---|
| RPG Maker MV / MZ | `.rpgsave`, `.rmmzsave` | |
| Unity · Easy Save 3 | `.es3` | plain, GZip, and AES-encrypted (password needed) |
| Unreal Engine | `.sav` (GVAS) | |
| Palworld | `.sav` (PlZ) | PlM (Oodle) not supported yet |
| Plain JSON | `.json` | |

## Platforms

Windows first. Linux and macOS builds come later.

## Command line

```
saveprism inspect <save>                                   # detected format and round-trip result
saveprism decode <save> -o save.json                       # save → JSON + save.json.recipe.toml
saveprism encode save.json --recipe <recipe> -o <save>     # JSON → save (backup first)
saveprism keys try <save> --candidates <list.txt>          # find the password of an encrypted ES3 save
```

## Documentation

Planning notes are written in Turkish; code, identifiers and commit messages are in English.

| File | Contents |
|---|---|
| [NOW.md](NOW.md) | Current state and the next step |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones and tasks |
| [docs/architecture/MIMARI.md](docs/architecture/MIMARI.md) | Architecture |
| [docs/architecture/kararlar.md](docs/architecture/kararlar.md) | Decision records (ADR) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to build and contribute |

## License

[MIT](LICENSE). Made for single-player saves of games you own. Online games, anti-cheat and DRM are out of scope.

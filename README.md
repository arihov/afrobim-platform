# AfroBIM

AfroBIM is a BIM platform for roads, written in Rust for Windows, macOS and Linux. It covers the
workflow from concept design to construction: alignment design, traffic signs and their supports,
road markings, drainage, pavement, road restraint, street lighting, gantries, corridor and
earthworks, clash detection, 4D programme, 5D cost, drawing sheets, and site progress.

## Layout

| Folder | Contents |
|---|---|
| `afrobim/` | The Rust workspace: every crate, the desktop app, the Python package, packaging and docs |
| `packs/` | Sign rule packs: UK Traffic Signs Manual and Uganda Traffic Signs Manual |
| `fonts/` | The Transport fonts the sign engine draws with (third-party files) |
| `Manuals/` | Reference documents the modules cite (Git LFS) |
| `docs/plan/` | Roadmap and build plans |

## Start here

- Build and run: `afrobim/docs/installing.md`
- What is built, with limits: `afrobim/README.md`
- User manual: `afrobim/docs/user-manual.md`

```
cd afrobim
cargo run --release -p afrobim-app -- --demo
```

## History

This repository was split out of `arihov/signcore` on 2026-11-15 with the commit history of the
files it contains. The .NET SignCore application stays in that repository.

## Notes

- No licence has been chosen yet. Until one is added, all rights are reserved by the owner.
- The Transport fonts and the reference manuals are third-party material. Do not redistribute them
  without a licence that allows it.

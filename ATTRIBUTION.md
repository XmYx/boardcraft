# Attribution

Every non-code asset in BoardCraft, with its author, source and licence. The rules are in
[AGENTS.md](AGENTS.md): no artwork from commercial storyboard tools, open licences only, every asset
listed here in the same change. `cargo xtask assets` checks that every file under `assets/`,
`docs/images/` and `docs/brand/` is listed.

## In this repository

| Asset | Author | Source | Licence | Used for |
|---|---|---|---|---|
| `assets/app-icon/boardcraft.svg` | BoardCraft contributors | Original work: draft app icon (heron in a storyboard frame) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, README |
| `assets/app-icon/hicolor/scalable/apps/ai.storyteller.boardcraft.svg` | BoardCraft contributors | Copy of `assets/app-icon/boardcraft.svg` under the Linux icon-theme name | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux desktop icon |
| `assets/app-icon/hicolor/16x16/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (16 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/24x24/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (24 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/32x32/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (32 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/48x48/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (48 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/64x64/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (64 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/128x128/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (128 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/256x256/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (256 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/hicolor/512x512/apps/ai.storyteller.boardcraft.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (512 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Linux icon theme, window icon |
| `assets/app-icon/boardcraft-1024.png` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (1024 px render) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Packaging |
| `assets/app-icon/boardcraft.ico` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (Windows icon, PNG entries 16–256 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Packaging |
| `assets/app-icon/boardcraft.icns` | BoardCraft contributors | Rendered from `assets/app-icon/boardcraft.svg` by `cargo xtask icons` (macOS icon, PNG entries 16–1024 px) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | Packaging |
| `docs/images/boardcraft-board.png` | BoardCraft contributors | Original work: BoardCraft screenshot (`--demo --screenshot`) of the built-in demo board, whose script and drawings are original work in `crates/core/src/demo.rs` | MIT OR Apache-2.0 | README |
| `docs/images/boardcraft-panel.png` | BoardCraft contributors | Original work: BoardCraft screenshot (`--demo --view panel --screenshot`) of the built-in demo board | MIT OR Apache-2.0 | README |

Code-drawn content (the demo screenplay *Paper Boats* and its procedural sketches in
`crates/core/src/demo.rs`) is original work under the project licence.

## Compiled in through dependencies

| Asset | Author | Source | Licence | Used for |
|---|---|---|---|---|
| Ubuntu Light (`Ubuntu-Light.ttf`) | Canonical Ltd. | egui's `epaint_default_fonts` crate | Ubuntu-font-1.0 | egui's interface face; board-sheet PDF text (subset) when no craft-fonts face is embedded |
| Hack Regular | Source Foundry Authors (DejaVu parts public domain) | `epaint_default_fonts` | MIT | egui's monospace face |
| Noto Emoji Regular | Google | `epaint_default_fonts` | OFL-1.1 | egui's emoji fallback |
| emoji-icon-font / egui-icons | John Slegers / egui authors | `epaint_default_fonts` | MIT | egui's icon fallback |

These ship with egui itself; no font file is in this repository.

## Embedded at build time from craft-fonts (`CRAFT_FONTS_DIR`)

Optional: with `CRAFT_FONTS_DIR` set, the faces selected by craft-fonts' `fonts/manifest.txt` are
embedded, with their licences listed in
[craft-fonts ATTRIBUTION.md](https://github.com/storytold/craft-fonts/blob/main/ATTRIBUTION.md)
(OFL-1.1). Without it, BoardCraft uses egui's default fonts.

## Embedded at build time from craft-i18n (`CRAFT_I18N_DIR`)

Optional: translation catalogs `locales/<locale>/common.tsv` and `boardcraft.tsv` from
[storytold/craft-i18n](https://github.com/storytold/craft-i18n) (MIT OR Apache-2.0). No BoardCraft
catalog exists yet; last synced craft-i18n SHA: none.

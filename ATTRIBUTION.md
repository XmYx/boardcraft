# Attribution

Every non-code asset in BoardCraft, with its author, source and licence. The rules are in
[AGENTS.md](AGENTS.md): no artwork from commercial storyboard tools, open licences only, every asset
listed here in the same change. `cargo xtask assets` checks that every file under `assets/`,
`docs/images/` and `docs/brand/` is listed.

## In this repository

| Asset | Author | Source | Licence | Used for |
|---|---|---|---|---|
| `assets/app-icon/boardcraft.svg` | BoardCraft contributors | Original work: draft app icon (heron in a storyboard frame) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, README |
| `docs/images/boardcraft-board.png` | BoardCraft contributors | Original work: BoardCraft screenshot (`--demo --screenshot`) of the built-in demo board, whose script and drawings are original work in `crates/core/src/demo.rs` | MIT OR Apache-2.0 | README |
| `docs/images/boardcraft-panel.png` | BoardCraft contributors | Original work: BoardCraft screenshot (`--demo --view panel --screenshot`) of the built-in demo board | MIT OR Apache-2.0 | README |

Code-drawn content (the demo screenplay *Paper Boats* and its procedural sketches in
`crates/core/src/demo.rs`) is original work under the project licence.

## Embedded at build time from craft-fonts (`CRAFT_FONTS_DIR`)

None yet. BoardCraft uses egui's default fonts until it adopts craft-fonts.

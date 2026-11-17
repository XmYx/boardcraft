# Contributing to BoardCraft

Read [AGENTS.md](AGENTS.md) first: the suite's house rules and BoardCraft's own rules (never
crash, clean room, assets, names, layering).

1. Rust 1.95 or newer through `rustup`.
2. `cargo run -p boardcraft -- --demo` to try the app.
3. `cargo xtask ci` before every commit (fmt, clippy `-D warnings`, tests, layering, assets).
4. Changes to `main` go through pull requests.
5. New assets go in [ATTRIBUTION.md](ATTRIBUTION.md) in the same change.
6. Screenshots for `docs/images/`: `cargo run -p boardcraft -- --demo --screenshot docs/images/<name>.png`.

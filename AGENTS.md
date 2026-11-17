<!-- craft-house-rules:start core source=279b2c09ef2b2f0b4f5e616a47d39e29db0a1eb3 sha256=bee0ffafad196b68c76425770e344e588463bbda685ab41581fcbabb85dfc930 -->
## House rules

These apply in every Craft repository. A later rule may be stricter, not weaker. The source is `standards/house-rules-core.md` in `storytold/craft-contrib`. Propose changes there, not in a copy.

1. **A person is accountable.** Whoever runs an agent answers for what it opens and for its review comments.
2. **Check, then claim.** Before writing code, read the issue comments, search pull requests for `#N` and the keywords, and check recent commits on `main`. Comment on the issue first. Release a claim you will not finish.
3. **Verify, don't guess.** Open a definition before using it. Confirm the cause in the code, even when someone names a location.
4. **Keep fixes narrow and tested.** Fix what the issue describes, where the bug is. Add a test that fails without the fix.
5. **Review the diff before opening a pull request.** Identifiers exist as used, the test fails without the fix, nothing unrelated changed, and the description is true.
6. **Look for a better pull request.** If another fixes the same thing, close yours when it is better or equal, and say why.
7. **Report only what ran.** Say what you tested and what you could not run. Without a local build or test, say so and rely on CI before marking ready.
8. **`main` changes through a pull request.** Do not push to it. Branch protection enforces this. There is no exception.
9. **Stop and ask only about** licensing outside the allowlist, publishing, releases, a new remote, secrets, spending money, destructive git, and anything else hard to undo. Otherwise keep working. `plan/` is a gitignored notebook, not rules. Without it, do not ask for it and do not stop.
10. **Keep secrets and personal details out** of the repository, logs and requests.
11. **CI stays shared.** Workflows call `storytold/craft-common-actions`. Do not change CI in a feature or fix pull request. If CI cannot cover a change, open an issue there.
12. **Guards say why.** A check that looks removable says what it prevents. Read that before changing it.
13. **Instructions must be readable by everyone.** No notes about one machine, and no links to files a reader cannot open. `craftrules` is retired. Do not link to it.
14. **Keep pull requests moving.** Concurrent pull requests are fine; unanswered actionable feedback is not. Respond in 7 days, a reminder at 14, closure eligible at 30, from when an actionable request waits on the author. Suggestions and unconfirmed bot findings do not start the clock. A fix, explanation or agreed plan resets it; unrelated activity does not. Waiting on a reviewer or maintainer pauses it and does not count against the author. A maintainer decides closure; it is never automatic. Reuse search results, do not poll, and stop at a rate limit.
15. **No agent attribution.** A pull request does not say an agent wrote it. A repository rule may not require that.
16. **Shared libraries first for Craft apps.** Before adding or changing reusable file-format, codec, protocol or UI code, check `storytold/craft-libs` and relevant open pull requests. Reuse its public APIs or extend the appropriate shared library instead of duplicating an implementation in an app. Keep app-specific policy and model mapping local; preserve behavior, security and resource limits, attribution, layering and regression coverage. Document API gaps and maintainer-approved exceptions rather than silently dropping behavior. This does not authorize dependencies on sibling application crates or override existing platform, licensing or dependency-source requirements.
<!-- craft-house-rules:end -->
<!-- craft-house-rules:start rust source=279b2c09ef2b2f0b4f5e616a47d39e29db0a1eb3 sha256=56ef2b873de68940cf903069bbd0927b119f5c9ec2717f7f627041d2ebe8e4f2 -->
## Rust Craft repositories

These apply to native Craft apps and to shared Rust libraries. Select them by repository type. A repository that merely contains some Rust does not take this block. Rules later in a repository's file may be stricter, but not weaker. The source is `standards/house-rules-rust.md` in [storytold/craft-contrib](https://github.com/storytold/craft-contrib).

1. **Never crash.** Non-test code does not panic and has no `unsafe`, except where the repository file names an exception. Return errors, treat input as hostile, and bound recursion and allocations. New production crate roots copy the `deny` attribute from an existing crate: `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::unimplemented`, `clippy::todo`, `clippy::unreachable`. Every crash fix comes with a test that panicked before the fix.
2. **Clean room.** Learn from observed behaviour and public specifications. Do not copy another product's code, assets, strings or artwork. Check a dependency's licence before adding it.
3. **Assets and fonts.** Every asset is original or suitably licensed, and is recorded in `ATTRIBUTION.md` in the same change. Font files live in [storytold/craft-fonts](https://github.com/storytold/craft-fonts), not in the app repository.
4. **Rust only for native Craft apps.** No handwritten JavaScript or TypeScript, and no Tauri, Electron or webview shell, in those apps. Shared tooling follows its documented technical scope and maintainer-approved exceptions. Craft PR Review's existing Tauri and TypeScript UI is one such exception. It does not relax this rule for native apps. Preserve any existing web build.
5. **Time goes through `chrono`.** Do not hand-roll calendar conversion. For dates beyond what `chrono` can represent, follow [standards/time.md](https://github.com/storytold/craft-contrib/blob/main/standards/time.md).
6. **Contributor credits.** Add or edit only your own person's entry in `contributors/people.toml`, and only after asking them. Never guess a name or hand-edit `contributors.json`.

## Licence allowlist

Dependencies and assets use SPDX identifiers. An expression is allowed when every identifier is in this list. A repository may name a shorter list. It may not add an identifier here by editing its own copy of these rules.

- `ISC`
- `MIT`
- `BSD-2-Clause`
- `BSD-3-Clause`
- `Apache-2.0`
- `Unicode-3.0`
- `OFL-1.1`
- `Ubuntu-font-1.0`
- `Bitstream-Vera`
- `CC0-1.0`
- `CC-BY-4.0`
- `CC-BY-SA-4.0`
- `Zlib`
- `BSL-1.0`
- `0BSD`
- `Unlicense`
- `IJG`
- `NCSA`
- `CDLA-Permissive-2.0`
- `Apache-2.0 WITH LLVM-exception`
- `LicenseRef-Public-Domain`

`MPL-2.0` is allowed for third-party dependencies used unmodified; vendoring, patching or copying its code needs a maintainer's approval. `MPL-2.0+` counts as `MPL-2.0`.

`LicenseRef-Storyteller-Trademark` is not in this list. It covers Storyteller brand marks only, where a repository already says so.
<!-- craft-house-rules:end -->

# BoardCraft — rules for agents and contributors

The generated blocks above are the suite-wide house rules from
[storytold/craft-contrib](https://github.com/storytold/craft-contrib) (`standards/house-rules-core.md`
and `standards/house-rules-rust.md`); never edit them by hand, regenerate them with the house-rules
action. The rules below are BoardCraft's own. When they disagree with the house rules, the house
rules win. `CLAUDE.md` only includes this file.

## 0. Never crash

People trust BoardCraft with days of drawing. A malformed project or screenplay, an odd pen event,
a huge image or a full disk must give an error the user can act on, never a crash and never lost
work.

1. Fail with `Result`; the UI reports errors in the status bar and carries on.
2. No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` outside tests. Every
   crate root carries the `#![deny(clippy::unwrap_used, …)]` line; `clippy.toml` allows them in tests.
3. No `unsafe` (`unsafe_code = "forbid"` for the workspace).
4. Every input-derived number is hostile: project JSON, PNG sizes, camera keys, durations, Fountain
   text, pointer coordinates and pressure. Clamp, validate (`Settings::is_valid`, `Project::repair`,
   `CameraKey::sanitized`) and cap allocations (`raster::MAX_SIDE`, the zip limits in `format.rs`).
5. Saves are atomic (`craft-atomic`): a failed save leaves the previous file intact.
6. Every parser has hostile-input tests (truncation, bit flips, absurd values) run under
   `catch_unwind`; a crash fix comes with a regression test.

## 1. Assets

No artwork, icons, screenshots, fonts or templates from Toon Boom, Adobe or any other commercial
storyboard tool, in any form (traced, redrawn or "inspired by" included). Every asset is openly
licensed or original work under the project licence and is listed in [`ATTRIBUTION.md`](ATTRIBUTION.md)
in the same change; `cargo xtask assets` checks it. Fonts live in
[storytold/craft-fonts](https://github.com/storytold/craft-fonts), never in this repository.
Screenshots in `docs/images/` show only BoardCraft with the built-in demo board (`--demo`).

## 2. Clean room

BoardCraft is a clean-room storyboarding and animatic app. Behaviour may be learned by using
commercial tools and from their public documentation; never read, disassemble or copy their code,
file formats' private internals or assets. File formats are implemented from public specifications:
Fountain (fountain.io), OpenTimelineIO JSON (opentimeline.io), ZIP (PKWARE APPNOTE). Dependencies use
permissive licences (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, CC0, MPL-2.0 unmodified).

## 3. Names

User-visible feature names are our own (craft-contrib `standards/naming.md`); no other company's
product or feature names as labels. The list is [`docs/feature-names.md`](docs/feature-names.md).
AI features are labelled "AI …", never with a vendor or model name.

## 4. Engineering

- **Pure Rust, shared libraries first.** Before writing reusable format, codec, protocol or UI code,
  check [storytold/craft-libs](https://github.com/storytold/craft-libs) (house rule 16, RFC-0040).
  BoardCraft uses `craft-zip`, `craft-image-codecs`, `craft-atomic` and `craft-ai-core` from there.
  The raster brush in `crates/core/src/raster.rs` is a candidate to move to craft-libs once a second
  app needs it.
- **Layering.** `boardcraft-core` (model, formats, import/export, brush) never depends on egui,
  eframe, winit or rfd; `cargo xtask layers` enforces it. The app (`apps/boardcraft`) is the only UI.
- **Accessibility.** Every control has an accessible name (AccessKit); everything works from the
  keyboard; UI tests use `egui_kittest`, whose `run()` fails on unnamed input widgets.
- **Toolchain.** Rust ≥ 1.95 (edition 2024). egui/eframe 0.37 from the suite's fork, pinned in the
  root `Cargo.toml` `[patch.crates-io]`, the same revisions as craft-libs.
- **Gates.** `cargo xtask ci` (fmt check, clippy `-D warnings`, tests, layering, assets) before every
  commit.
- **Build output** stays in the repository's `target/`.

## See also

- [README.md](README.md), [ROADMAP.md](ROADMAP.md), [docs/gaps.md](docs/gaps.md),
  [docs/target-app-parity.md](docs/target-app-parity.md): stage, parity and the ranked work list
- [docs/architecture.md](docs/architecture.md): crates, data model, rendering
- [docs/file-format.md](docs/file-format.md): the `.boardcraft` file
- [docs/interop.md](docs/interop.md): ScriptCraft (Fountain) in, FilmCraft (OTIO) out

# Roadmap detail

Plans by milestone. The stage and headline numbers are in [ROADMAP.md](../ROADMAP.md); the ranked
work list is [gaps.md](gaps.md).

| Milestone | Goal | Contents | Status |
|---|---|---|---|
| M0 Skeleton | adoptable repository | model, `.boardcraft`, Fountain in, OTIO/PNG out, commands + CLI, egui app, i18n/fonts wiring, accessibility tests, packaging, CI callers | done (this commit) |
| M1 Boards people can hand out | printable output, safe work | PDF board sheets, shot list, duplication, full undo, autosave and recovery, Board Settings (all done); sheet options | nearly done |
| M2 Animatic into the edit | camera moves survive | camera keys as FilmCraft Motion keyframes, scratch audio track, transitions | planned |
| M2b Sound | animatic with sound | audio track, waveform, movie export with burn-ins | next (gaps #1–2) |
| M3 Drawing | real sketching | onion skin, zoom/pan, straight lines, external editor (done); pressure curve, stabilizer, lasso/transform, rotation, fill, vector | in progress |
| M4 Script round trip | ScriptCraft stays in sync | re-import by `[[scene-id]]` with a report, Final Draft and scene-list JSON import, conform timing from the edit | done |
| M5 Agents | headless and MCP | MCP server over the command layer, control channel | planned |
| M6 AI | AI panel sketch | craft-ai-client wiring, consent, progress, result as a layer | stubbed |

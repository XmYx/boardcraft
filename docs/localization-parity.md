# Localization parity

How far BoardCraft's interface is ready for other languages.

| Area | State |
|---|---|
| Lookup | every interface string in `apps/boardcraft` goes through `tl!`/`tlf!` (`craft-i18n` crate, craft-libs) |
| Catalogs | `build.rs` embeds `CRAFT_I18N_DIR/locales/<locale>/{common,boardcraft}.tsv`; none exist for BoardCraft yet, so the app runs in English |
| Releases | `CRAFT_I18N_REQUIRED=1` makes a missing checkout a build error |
| Language picker | View ▸ Language: System, English, and every offered locale that has a `boardcraft.tsv` |
| Offered locales | de, es, fr, id, it, ja, ko, nl, pl, pt-br, ru, uk, zh-hans, zh-hant (left-to-right only until the layout mirrors) |
| Placeholders | `{name}` templates through `craft_i18n::fmt` (isolated for bidi) |
| Fonts | craft-fonts faces through `CRAFT_FONTS_DIR` (`craft-fonts-build`) as fallbacks for scripts egui's fonts lack |
| Source list | `cargo xtask strings <craft-i18n checkout>` writes [i18n/boardcraft-strings.tsv](i18n/boardcraft-strings.tsv): 122 source texts, 28 of them already in the shared `common.tsv` (marked `shared`) |
| Not translated yet | error messages from `boardcraft-core` (English `Display` texts), file-dialog filter names on some platforms, the demo board's script |
| Right to left | not offered: canvas, timeline and panel grid do not mirror yet |
| Numbers | durations and camera values use `.` decimals; `craft_i18n::Format` adoption is a gap |

## Shared strings BoardCraft uses

The `shared` rows of [i18n/boardcraft-strings.tsv](i18n/boardcraft-strings.tsv) are the source texts
craft-i18n's `common.tsv` already translates (Save, Open…, Cancel, Delete, Edit, View, …); BoardCraft's
own catalog only needs the `app` rows.

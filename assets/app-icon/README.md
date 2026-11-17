# BoardCraft app icon

`boardcraft.svg` is the canonical icon (a heron standing in a storyboard frame above a panel strip)
in the suite's three-colour style: Ink `#0b0b0c`, Paper `#f2f1ee` and the BoardCraft colour
`#e26842`, on a 512 tile with `rx=112` and the art clipped at `rx=106`. It is a contributor draft;
the ArtCraft team may replace it with an engraved-creature icon like the other Crafting Apps'.

Every other file here is rendered from it by `cargo xtask icons` (craft-svg's bounded renderer and
craft-image-codecs), committed so packaging needs no tools:

- `hicolor/{16,24,32,48,64,128,256,512}x…/apps/ai.storyteller.boardcraft.png`, plus
  `hicolor/scalable/apps/ai.storyteller.boardcraft.svg` (Linux icon theme; the 256 px PNG is also the
  window icon);
- `boardcraft-1024.png`;
- `boardcraft.ico` (Windows, PNG entries 16–256 px) and `boardcraft.icns` (macOS, PNG entries
  16–1024 px).

After changing the SVG, run `cargo xtask icons` and commit the results.

# Lightning Librarian

Lightning Librarian is a desktop reading companion for managing EPUB content with a shared Rust backend (`reader-core` + `ui` crates) and a CLI/GUI wrapper under the `librarian` crate.

## Example usage

```bash
# build everything and run the reader UI
cargo run -p librarian
```

The UI is powered by a native window (SDL/gtk/egui depending on configuration), so the binary will launch a reader view where you can open EPUB files from the `docs/` directory or point the file picker to your own collection.

## Controls

- **Open file**: use the `File → Open` menu or drag & drop an EPUB onto the window.
- **Navigation**: click through chapters in the sidebar or use the arrow keys (←/→) to move between pages.
- **Search**: type in the search field and hit Enter to filter the current document.
- **Settings**: open the preferences (gear icon) to tweak font size, theme, and pagination mode.
- **Book layout**: press `b` to toggle a two-page spread view.

## Git hooks

Install [lefthook](https://github.com/evilmartians/lefthook)

Then run `lefthook install` in the repo root; it will run `cargo fmt && cargo clippy` before commits and `cargo test` before pushes.

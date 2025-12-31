# AGENTS.md

## Commands

- Build: `cargo build` (workspace)
- Run: `cargo run -p librarian`
- Test: `cargo test` (workspace); single test: `cargo test -p reader-core normalize::tests::html_to_blocks_basic`
- Lint: `cargo clippy --all-targets -- -D warnings`; format: `cargo fmt --all`

## Rust Configuration

- Edition: 2021; stable toolchain
- Keep dependencies minimal and up-to-date
- Workspace structure: `librarian/` (binary), `crates/reader-core/`, `crates/ui/`, `crates/highlight/`

## Imports

- Group imports in order: std library, external crates, internal modules
- Prefer explicit `use` over glob imports (`use crate::types::Block` not `use crate::types::*`)
- Re-exports go at module bottom: `pub use layout::extract_words;`

## Formatting

- Run `cargo fmt --all` before committing
- Target 100‑char line width (check with clippy)
- Use trailing commas in multi-line lists/structs
- Avoid needless `mut` - declare as const then add `mut` only when needed

## Types

- Prefer `&str` over `String` in public APIs; only allocate when necessary
- Use `Option<T>` and `Result<T, E>` idiomatically; prefer `?` propagation
- Avoid `unwrap()`/`expect()` outside tests; use `?`, `.unwrap_or_default()`, or `.ok()?`
- Clone sparingly; prefer borrowing or returning references where feasible

## Naming

- Functions, variables, modules: `snake_case`
- Types, enums, structs: `CamelCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Keep module names concise and descriptive (e.g., `normalize`, `layout`)

## Error Handling

- Use `thiserror` crate for error enums: `#[derive(Debug, Error)]`
- Bubble errors with `?` operator; return contextual error messages
- Custom error variants should describe the failure cause clearly
- Pattern match on errors for user-facing messages (see `librarian/src/main.rs:312-319`)

## Performance

- Avoid allocations in hot paths; use iterators and slices
- Cache layouts when recomputing would be expensive
- Use `RefCell` sparingly; prefer `Mutex` for concurrent access
- Pre-allocate `Vec` capacity when size is known: `Vec::with_capacity(n)`

## TUI / Rendering

- ratatui components: clear separation of state vs render logic
- No blocking I/O in draw loop; poll channels with `try_recv()`
- Use `term_size` or terminal events for layout calculations
- Implement `reflow()` to recompute layout on terminal resize

## Testing

- Unit tests per module in `#[cfg(test)]` blocks
- Use fixtures under `tests/` directory for integration tests
- Test error paths and edge cases, not just happy paths
- Keep tests fast and independent

## Concurrency & Streams

- Use `mpsc::channel` for async document loading (epub/pdf streaming)
- Spawn worker threads with `thread::spawn` for background loading
- Use `try_recv()` in event loop to avoid blocking
- Prefetch windows configurable via env vars

## HTML / Text Processing

- Sanitize HTML/CSS inputs; extract structured content only
- Preserve inline markup (bold, italic, links) using marker chars
- Handle Unicode grapheme clusters correctly (`unicode-segmentation` crate)
- Strip zero-width and invisible characters

## Commit & PR Guidelines

- Small, focused changes per commit
- Run `cargo fmt`, `cargo clippy`, and `cargo test` before pushing
- Update docs when public APIs change
- Security: do not add DRM support; avoid unsafe unless justified
- Lefthook runs `cargo fmt && cargo clippy` on pre-commit, `cargo test` on pre-push

## Environment Variables

- `LIBRARIAN_PDF_BACKEND`: "pdf-rs" (default) or "lopdf"
- `LIBRARIAN_PDF_PAGE_LIMIT`: limit loaded pages (0 = all)
- `LIBRARIAN_PDF_PREFETCH_PAGES`: prefetch window (default 2)
- `LIBRARIAN_EPUB_INITIAL_CHAPTERS`: initial chapters to load (default 1)
- `LIBRARIAN_EPUB_PREFETCH_CHAPTERS`: prefetch window (default 2)

## Config Files

- Config root: `~/.config/librarian/` with legacy fallbacks
- `config.toml`: theme settings (header/footer colors, presets)
- `settings.toml`: view preferences (justify, two_pane, spritz_wpm)
- State files: `book-state/<book-id>.toml` for reading position

## External Tools Integration

- Cursor/Copilot rules: if found in `.cursor/rules/`, `.cursorrules`, or `.github/copilot-instructions.md`, follow them and surface conflicts in PRs

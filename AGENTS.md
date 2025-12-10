# AGENTS.md

- Build: `cargo build` (workspace)
- Run: `cargo run -p librarian`
- Test: `cargo test` (workspace); single test: `cargo test -p reader-core normalize::tests::html_to_blocks_basic`
- Lint: `cargo clippy --all-targets -- -D warnings`; format: `cargo fmt --all`

- Rust edition: 2021; stable toolchain. Keep dependencies minimal and up-to-date.
- Imports: group std, external crates, internal modules; prefer explicit `use` over glob imports.
- Formatting: run `cargo fmt`; 100‑char line width; trailing commas; avoid needless `mut`.
- Types: prefer `&str` over `String` in APIs; use `Option`/`Result` idiomatically; avoid `unwrap`/`expect` outside tests.
- Naming: snake_case for functions/vars, CamelCase for types; modules concise and descriptive.
- Errors: use `thiserror` for error enums; bubble errors with `?`; return contextual messages.
- Performance: avoid allocations in hot paths; cache layouts; prefer iterators and slices; avoid cloning.
- TUI: ratatui components with clear separation of state vs render; no blocking IO in draw loop.
- Testing: unit tests per module; snapshot tests for rendering; use fixtures under `tests/`.
- Commit scope: small, focused changes; follow workspace style; update docs when APIs change.
- Security: do not add DRM support; sanitize HTML/CSS inputs; avoid unsafe unless justified.
- If Cursor/Copilot rules exist (`.cursor/`, `.cursorrules`, `.github/copilot-instructions.md`), follow them and surface conflicts in PRs.

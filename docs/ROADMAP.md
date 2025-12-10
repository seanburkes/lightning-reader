# Librarian Roadmap

## Goals
- Focused reading: adjustable narrow column width
- Dual-pane: sequential pages of same chapter
- Code sections: zoom, syntax highlight, copy-on-select
- Speed reading: RSVP pivot highlight, adjustable WPM
- Terminal-first (tmux/zellij/ssh), vi-style keybindings
- Primary format: EPUB (no DRM); PDF later

## Tech Stack (Rust stable)
- TUI: ratatui + crossterm
- EPUB/HTML: zip + OPF/NCX parsing, html5ever/kuchiki
- Text: unicode-linebreak, hyphenation (TeX patterns)
- Syntax highlight: syntect (themes: Gruvbox, Dracula, TokyoNight)
- Clipboard: OSC 52 + system fallbacks
- Search: in-book initially; tantivy later for global
- Config: serde + toml; app dirs via directories

## Architecture
- reader-core: EPUB ingest → DOM normalize → Block model → paginate
- ui: App state, views (Reader, TOC, Search, Code, Status), input
- highlight: syntect integration + theme management
- indexer (later): library metadata, tantivy

## Milestones
- M1: Basic EPUB reader
  - Open EPUB, parse spine, load chapter HTML
  - Normalize to blocks (paragraphs, headings, lists, code, quotes)
  - Greedy wrap with unicode line breaks; adjustable column width
  - Single pane render; vi navigation (h/j/k/l, gg/G, / stub)
  - TOC view; bookmarks; status bar with progress
  - Powerline-style header/footer bars inside reader area (width-fit)
- M2: Dual-pane sequential pages
  - Two synchronized panes; resize-aware reflow; layout cache
- M3: Code experience
  - Detect <pre><code class="lang-...">, syntect highlight
  - Zoom modal; copy-on-select via OSC 52; smooth scroll
- M4: Speed reading (RSVP)
  - Pivot highlighting; WPM control; punctuation pauses; quick backtrack
- M5: Search + indexing
  - In-book search; results navigation; global index with tantivy later
- M6: Settings + themes
  - Dark/light/solarized; Gruvbox/Dracula/TokyoNight; keymap config; hyphenation
- M7: PDF basic support
  - Text extraction, fixed layout scroll; experimental columns

## Key Design Decisions
- Minimal CSS mapping (bold/italic/underline/colors; headings; lists); ignore floats/grids
- Footnotes/endnotes as links/popup
- Lazy parse/render per chapter; cache layouts by width; debounce resize
- OSC 52 primary; explicit copy command; fallbacks for terminal variability
- Headers/footers rendered inside content area to avoid overflow; left/right segments with truncation and padding to fit exactly

## Testing Strategy
- Unit: HTML normalization; wrap/hyphenation
- Snapshot: rendered blocks across widths (40/60/80)
- Integration: large EPUBs; TOC/navigation correctness
- Terminal behavior: OSC 52 and mouse in tmux/zellij/ssh

## Samples
- Project Gutenberg: Alice’s Adventures in Wonderland (EPUB/EPUB3). Place under `samples/` for validation of prose, poetry, images.

## Current Status
- Workspace scaffolded: reader-core, ui, highlight crates; librarian binary
- Next: Define EPUB ingest APIs; normalization rules; layout paginate; TUI single column

## Next Actions
- Implement `EpubBook::open`, OPF/NCX parsing, chapter loader
- Normalize HTML → `Vec<Block>`; map tags/styles to TUI attributes
- Greedy paginate with unicode line breaks; add hyphenation toggle
- Render single column; vi keybindings; status bar; TOC stub
- Add theme files (Gruvbox/Dracula/TokyoNight) into assets

# Spritz (RSVP) Speed Reading Mode Implementation Plan

## Overview
Add Rapid Serial Visual Presentation (RSVP) speed reading mode using the Spritz algorithm, allowing users to read at adjustable WPM with pivot-point highlighting.

## Configuration
- **Default WPM**: 250
- **Content scope**: Prose only (skip code blocks, images)
- **Punctuation pauses**: Brief pauses following standard Spritz algorithm
- **Progress**: Visual progress bar for current chapter
- **Pivot highlighting**: ORP (Optimal Recognition Point) ~35% from word start

## Spritz Algorithm
The Spritz algorithm displays one word at a time with the "optimal recognition point" (ORP) highlighted:

1. **ORP Position**: For words 1-13 characters, pivot at ~35% from left
2. **Display format**:
   - Characters before ORP: Gray, dimmed
   - ORP character: Red/bold, centered
   - Characters after ORP: Black/normal
3. **Context**: Show only ~1-2 characters before/after pivot, not full words
4. **Punctuation pauses** (automatic):
   - Longer pause: `.`, `!`, `?`, `;`, `:`
   - Brief pause: `,`, `)`, `-`

## Implementation Steps

### Step 1: Add Spritz Mode Enum and Settings Persistence
**Files**: `crates/ui/src/app.rs`, `crates/reader-core/src/config.rs`

- Add `Spritz` variant to `Mode` enum
- Add `SpritzSettings` struct with:
  - `wpm: u16` (default 250, range 100-1000)
  - `pause_on_punct: bool` (default true)
  - `punct_pause_ms: u16` (default 100)
- Extend `load_settings()` and `save_settings()` to handle spritz config
- Store in `settings.toml` under `[spritz]` section

**Commit**: Add spritz mode enum and settings structure

---

### Step 2: Word Extraction from Blocks
**Files**: `crates/reader-core/src/layout.rs` (or new `reader_core/src/spritz.rs`)

- Create function `extract_words(blocks: &[Block]) -> Vec<WordToken>`
- Define `WordToken` struct:
  - `text: String`
  - `is_sentence_end: bool` (for `.`, `!`, `?`)
  - `is_comma: bool` (for `,`, etc.)
  - `chapter_index: Option<usize>` (for chapter boundaries)
- Process blocks:
  - `Paragraph`, `Heading`, `List`, `Quote`: Extract words
  - `Code`: Skip entirely (skip all content)
  - Images: Skip `[image]` placeholders
- Use existing `split_whitespace()` logic from `wrap_text()`
- Preserve punctuation attached to words for pause detection

**Commit**: Implement word extraction from prose blocks (skip code)

---

### Step 3: Create SpritzView Component
**File**: `crates/ui/src/spritz_view.rs` (new)

Create `SpritzView` struct:
```rust
pub struct SpritzView {
    words: Vec<WordToken>,
    current_index: usize,
    wpm: u16,
    is_playing: bool,
    last_update: std::time::Instant,
    settings: SpritzSettings,
    chapter_titles: Vec<String>,
}
```

Methods:
- `new(words: Vec<WordToken>, settings: SpritzSettings) -> Self`
- `play()`, `pause()`, `toggle_play()`
- `rewind(steps: usize)`, `fast_forward(steps: usize)`
- `adjust_wpm(delta: i16)`
- `get_orp_position(word: &str) -> usize` (standard Spritz algorithm)
- `update(&mut self) -> bool` (advance word based on WPM, returns true if word changed)

**Commit**: Create SpritzView component with playback controls

---

### Step 4: Implement Spritz Rendering
**File**: `crates/ui/src/spritz_view.rs`

Add `render()` method to display:
- Single word centered with pivot highlighting
  - Left chars: Gray/dim
  - Pivot char: Red/bold
  - Right chars: Normal
- Progress bar for current chapter
  - Visual bar using unicode block characters: `▮▯▯▯▯▯`
  - Show "Chapter X: Y%"
- Status line:
  - WPM: `250 WPM`
  - Word count: `Word 123/4567`
  - Playing state: `▶ Playing` or `⏸ Paused`
- Chapter title in header

**Commit**: Implement spritz view rendering with progress bar

---

### Step 5: Integrate Spritz Mode in App
**Files**: `crates/ui/src/app.rs`, `crates/ui/src/lib.rs`

- Add `spritz: Option<SpritzView>` field to `App`
- Add `mode: Mode` field (already exists, extend with `Spritz`)
- Export `spritz_view` module from `ui/src/lib.rs`
- Update `run()` main loop:
  - Check for mode switch: `Mode::Reader` ↔ `Mode::Spritz`
  - In `Mode::Spritz`: call `spritz_view.update()` and `spritz_view.render()`
- On mode enter: Extract words from blocks, create SpritzView
- On mode exit: Save settings, return to Reader mode

**Commit**: Integrate spritz mode into app event loop

---

### Step 6: Add Key Bindings for Spritz
**File**: `crates/ui/src/app.rs`

Add key handlers in `run()` event loop when `self.mode == Mode::Spritz`:
- `s` / `Esc`: Exit spritz mode (return to Reader)
- `Space`: Toggle play/pause
- `j` / `k` or `Left` / `Right`: Navigate word-by-word (rewind/forward)
- `Ctrl+j` / `Ctrl+k`: Jump 10 words
- `+` / `=`: Increase WPM (+10)
- `-` / `_`: Decrease WPM (-10)
- `r`: Rewind to chapter start
- `f`: Fast forward to chapter end
- `[` / `]`: Decrease/increase WPM by 50
- `Enter`: Resume playing if paused
- `?`: Show help overlay

**Commit**: Add spritz key bindings and controls

---

### Step 7: Implement Timing Loop
**File**: `crates/ui/src/app.rs`

In the main `run()` loop:
- When `Mode::Spritz` and `is_playing`:
  - Calculate delay per word: `delay_ms = 60000 / wpm`
  - Add extra pause for punctuation: `if is_sentence_end { delay_ms += punct_pause_ms }`
  - Use `std::time::Instant` to track elapsed time
  - Auto-advance word index when time elapsed
- Ensure non-blocking: check `event::poll()` with appropriate timeout
- Update display on every word change

**Commit**: Implement timing loop for word progression

---

### Step 8: Navigation Transitions Between Modes
**File**: `crates/ui/src/app.rs`

Implement bidirectional mapping:
- **Reader → Spritz**:
  - Extract words from current chapter's blocks
  - Find word index nearest to current page/line
  - Initialize SpritzView at that position
- **Spritz → Reader**:
  - Map word index → page index (using chapter_starts + word count per page)
  - Set `view.current` to nearest page
  - Return to Reader mode
- Handle edge cases:
  - Word at chapter boundary
  - Word not on any page (rare)

**Commit**: Add mode transition mapping (word ↔ page index)

---

### Step 9: Add Help Overlay for Spritz
**File**: `crates/ui/src/app.rs`

Reusing existing `show_help` overlay:
- When `Mode::Spritz` and `show_help`:
  - Display spritz-specific controls:
    ```
    Spritz Mode Help
    q / Ctrl-C / s / Esc: exit spritz mode
    Space: toggle play/pause
    j/k or ←/→: navigate word-by-word
    Ctrl+j/Ctrl+k: jump 10 words
    +/-: adjust WPM by 10
    [ / ]: adjust WPM by 50
    r: rewind to chapter start
    f: fast forward to chapter end
    ?: toggle this help
    ```
- Close with `?` or `Esc`

**Commit**: Add spritz help overlay

---

### Step 10: Testing and Polish
**Files**: `crates/ui/src/spritz_view.rs`, `crates/reader-core/src/layout.rs`

Add unit tests:
- Word extraction edge cases
  - Punctuation attachment
  - Empty paragraphs
  - Skipping code blocks
- ORP position calculation
  - Words 1-13 characters
  - Longer words
- Timing calculations
  - WPM → delay_ms conversion
  - Punctuation pause logic

Add integration testing:
- Mode transitions
- Settings persistence
- Key binding handlers

Polish:
- Ensure smooth rendering (no flicker)
- Progress bar visual consistency
- Handle empty documents gracefully

**Commit**: Add tests and polish spritz implementation

---

### Step 11: Documentation
**Files**: `README.md`, `docs/ROADMAP.md`

Update documentation:
- Add spritz mode to README controls section:
  ```
  Spritz speed reading:
    s: toggle spritz mode
    Space: play/pause
    j/k: navigate words
    +/-: adjust WPM
  ```
- Update ROADMAP to mark M4 as complete
- Add reference to this document

**Commit**: Update documentation for spritz mode

---

## Testing Checklist

- [ ] Toggle spritz mode with `s`
- [ ] Play/pause with Space
- [ ] Navigate words with j/k
- [ ] Adjust WPM (+/-, [/])
- [ ] Rewind to chapter start with `r`
- [ ] Fast forward to chapter end with `f`
- [ ] Progress bar displays correctly
- [ ] Pivot highlighting follows Spritz algorithm
- [ ] Punctuation pauses work automatically
- [ ] Code blocks are skipped
- [ ] Settings persist across sessions
- [ ] Mode transitions preserve reading position
- [ ] Help overlay displays correctly
- [ ] Exit spritz mode returns to Reader at correct location

## Future Enhancements (Post-MVP)

- Word-by-word speed adjustment (slow down on complex words)
- Context preview (show 1-2 surrounding words dimmed)
- Bookmarking specific word positions
- Reading statistics (words/minute, completion time)
- Custom ORP position configuration
- Multiple highlight themes (pivot color)
- Support for EPUB page mapping during spritz

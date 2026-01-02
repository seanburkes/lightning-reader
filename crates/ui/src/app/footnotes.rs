use reader_core::types::Block as ReaderBlock;

const STYLE_START: char = '\x1E';
const STYLE_END: char = '\x1F';
const LINK_START: char = '\x1C';
const LINK_END: char = '\x1D';
const ANCHOR_START: char = '\x18';
const ANCHOR_END: char = '\x17';

pub(super) fn find_footnote_text(
    blocks: &[ReaderBlock],
    target: &str,
    label: Option<&str>,
) -> Option<String> {
    for block in blocks {
        match block {
            ReaderBlock::Paragraph(text) | ReaderBlock::Quote(text) => {
                if let Some(note) = extract_footnote_from_text(text, target, label) {
                    return Some(note);
                }
            }
            ReaderBlock::List(items) => {
                for item in items {
                    if let Some(note) = extract_footnote_from_text(item, target, label) {
                        return Some(note);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn is_footnote_link(target: &str, label: Option<&str>) -> bool {
    if is_backlink_label(label) {
        return false;
    }
    let frag = target.split('#').nth(1).unwrap_or("");
    let frag_lower = frag.to_ascii_lowercase();
    let frag_note = frag_lower.contains("footnote")
        || frag_lower.contains("noteref")
        || frag_lower.contains("note")
        || frag_lower.starts_with("fn")
        || frag_lower.starts_with("note");
    frag_note || is_short_marker_label(label)
}

fn extract_footnote_from_text(text: &str, target: &str, label: Option<&str>) -> Option<String> {
    let (found, captured) = extract_after_anchor(text, target);
    if !found {
        return None;
    }
    let mut note = clean_footnote_text(&captured, label);
    if note.is_empty() {
        let fallback = clean_footnote_text(&strip_inline_markers(text), label);
        if !fallback.is_empty() {
            note = fallback;
        }
    }
    if note.is_empty() {
        None
    } else {
        Some(note)
    }
}

fn extract_after_anchor(text: &str, target: &str) -> (bool, String) {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut capturing = false;
    let mut found = false;
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let _ = chars.next();
            continue;
        }
        if ch == LINK_START {
            skip_until(&mut chars, LINK_END);
            continue;
        }
        if ch == ANCHOR_START {
            let anchor = read_until(&mut chars, ANCHOR_END);
            if !found && anchor == target {
                capturing = true;
                found = true;
            }
            continue;
        }
        if capturing {
            out.push(ch);
        }
    }
    (found, out)
}

fn read_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, end: char) -> String {
    let mut out = String::new();
    for ch in chars.by_ref() {
        if ch == end {
            break;
        }
        out.push(ch);
    }
    out
}

fn skip_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, end: char) {
    for ch in chars.by_ref() {
        if ch == end {
            break;
        }
    }
}

fn strip_inline_markers(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let _ = chars.next();
            continue;
        }
        if ch == LINK_START {
            skip_until(&mut chars, LINK_END);
            continue;
        }
        if ch == ANCHOR_START {
            skip_until(&mut chars, ANCHOR_END);
            continue;
        }
        out.push(ch);
    }
    out
}

fn clean_footnote_text(text: &str, label: Option<&str>) -> String {
    let mut s = normalize_note_text(text);
    if is_short_marker_label(label) {
        s = strip_leading_marker(&s);
    }
    s = strip_backlink(&s);
    s
}

fn normalize_note_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn strip_leading_marker(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut dropped = false;
    while let Some(ch) = chars.peek().copied() {
        if ch.is_ascii_digit() || is_note_symbol(ch) {
            dropped = true;
            let _ = chars.next();
            continue;
        }
        if dropped && matches!(ch, '.' | ')' | ':' | ']' | '[') {
            let _ = chars.next();
            continue;
        }
        if dropped && ch.is_whitespace() {
            let _ = chars.next();
            continue;
        }
        break;
    }
    if dropped {
        chars.collect::<String>().trim_start().to_string()
    } else {
        text.to_string()
    }
}

fn strip_backlink(text: &str) -> String {
    let mut s = text.trim().to_string();
    for suffix in ["↩︎", "↩", "⤴︎", "⤴", "↵"] {
        s = s.trim_end_matches(suffix).trim().to_string();
    }
    let lower = s.to_ascii_lowercase();
    for word in ["back", "return"] {
        if lower.ends_with(word) {
            let new_len = s.len().saturating_sub(word.len());
            s.truncate(new_len);
            s = s
                .trim_end_matches(|c: char| c.is_whitespace() || c == '(' || c == ')')
                .trim_end()
                .to_string();
            break;
        }
    }
    s
}

fn is_backlink_label(label: Option<&str>) -> bool {
    let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let lower = label.to_ascii_lowercase();
    lower == "back" || lower == "return" || lower == "↩" || lower == "↩︎"
}

fn is_short_marker_label(label: Option<&str>) -> bool {
    let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    if label.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if label.len() <= 2 && label.chars().all(is_note_symbol) {
        return true;
    }
    false
}

fn is_note_symbol(ch: char) -> bool {
    matches!(ch, '*' | '†' | '‡' | '§' | '¶')
}

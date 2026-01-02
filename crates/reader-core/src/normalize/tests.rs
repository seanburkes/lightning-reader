use crate::types::Block;

use super::inline::{
    normalize_line, ANCHOR_END, ANCHOR_START, LINK_END, LINK_START, STYLE_END, STYLE_START,
};
use super::{html_to_blocks, postprocess_blocks};

fn strip_inline_markers(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let _ = chars.next();
            continue;
        }
        if ch == LINK_START {
            for next in chars.by_ref() {
                if next == LINK_END {
                    break;
                }
            }
            continue;
        }
        if ch == ANCHOR_START {
            for next in chars.by_ref() {
                if next == ANCHOR_END {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[test]
fn preserves_dom_order() {
    let html = r#"
        <h1>Title</h1>
        <p>Intro text.</p>
        <ul><li>One</li><li>Two</li></ul>
        <p>Tail.</p>
        "#;
    let blocks = html_to_blocks(html);
    assert!(matches!(blocks[0], Block::Heading(ref t, 1) if t == "Title"));
    assert!(matches!(blocks[1], Block::Paragraph(ref t) if t == "Intro text."));
    assert!(matches!(blocks[2], Block::List(ref items) if items == &["One", "Two"]));
    assert!(matches!(blocks[3], Block::Paragraph(ref t) if t == "Tail."));
}

#[test]
fn preserves_inline_markup_and_links() {
    let html = r#"
        <p>Read <em>this</em> and <strong>that</strong>, see
        <a href="https://example.com">link</a>.</p>
        "#;
    let blocks = html_to_blocks(html);
    let Block::Paragraph(text) = &blocks[0] else {
        panic!("expected paragraph");
    };
    assert!(text.contains(STYLE_START));
    let stripped = strip_inline_markers(text);
    assert!(matches!(
        blocks[0],
        Block::Paragraph(ref t)
            if strip_inline_markers(t)
                == "Read this and that, see link (https://example.com)."
    ));
    assert_eq!(
        stripped,
        "Read this and that, see link (https://example.com)."
    );
}

#[test]
fn extracts_tables_as_table_blocks() {
    let html = r#"
        <table>
          <tr><th>Head</th><th>Value</th></tr>
          <tr><td>A</td><td>B</td></tr>
        </table>
        "#;
    let blocks = html_to_blocks(html);
    let Block::Table(table) = &blocks[0] else {
        panic!("expected table block");
    };
    let rows = table.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 2);
    assert_eq!(rows[0][0].text(), "Head");
    assert_eq!(rows[0][1].text(), "Value");
    assert_eq!(rows[1][0].text(), "A");
    assert_eq!(rows[1][1].text(), "B");
}

#[test]
fn preserves_line_breaks_from_br() {
    let html = r#"<p>Line one<br/>Line two</p>"#;
    let blocks = postprocess_blocks(html_to_blocks(html));
    assert!(matches!(
        blocks[0],
        Block::Paragraph(ref t) if t == "Line one\nLine two"
    ));
}

#[test]
fn captures_orphan_text_outside_blocks() {
    let html = r#"<div>Loose <em>text</em> without closing"#;
    let blocks = html_to_blocks(html);
    let Block::Paragraph(text) = &blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(strip_inline_markers(text), "Loose text without closing");
}

#[test]
fn removes_soft_hyphens_in_flow() {
    let input = "co\u{00AD}operate re-enter";
    assert_eq!(normalize_line(input), "cooperate re-enter");
}

#[test]
fn captures_code_with_language() {
    let html = r#"<pre><code class="rust">fn main() {}</code></pre>"#;
    let blocks = html_to_blocks(html);
    assert!(matches!(
        blocks[0],
        Block::Code { ref lang, ref text }
            if lang.as_deref() == Some("rust") && text.contains("fn main")
    ));
}

use std::io::Write;

use lopdf::{
    content::{Content, Operation},
    dictionary, Object, Stream,
};
use reader_core::pdf::{load_pdf, load_pdf_with_limit, PdfLoader};

fn build_test_pdf(pages: &[&str]) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.new_object_id();
    doc.objects.insert(
        font_id,
        Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }),
    );

    let mut page_ids = Vec::new();
    for text in pages {
        let mut content = Content {
            operations: Vec::new(),
        };
        content.operations.push(Operation::new("BT", vec![]));
        content.operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 12.into()],
        ));
        content
            .operations
            .push(Operation::new("Td", vec![50.into(), 150.into()]));
        content.operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(text.to_string())],
        ));
        content.operations.push(Operation::new("ET", vec![]));
        let content_bytes = content.encode().unwrap_or_default();
        let content_id = doc.new_object_id();
        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(dictionary! {}, content_bytes)),
        );

        let resources_id = doc.new_object_id();
        doc.objects.insert(
            resources_id,
            Object::Dictionary(dictionary! {
                "Font" => dictionary! { "F1" => Object::Reference(font_id) }
            }),
        );

        let page_id = doc.new_object_id();
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Reference(resources_id),
            }),
        );
        page_ids.push(page_id);
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(page_ids.len() as i64),
            "Kids" => page_ids.iter().cloned().map(Object::Reference).collect::<Vec<_>>(),
        }),
    );

    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }),
    );

    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.trailer.set(
        "Info",
        dictionary! {
            "Title" => Object::string_literal("Test Title"),
            "Author" => Object::string_literal("Test Author"),
        },
    );
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save pdf");
    buf
}

#[test]
fn pdf_loader_extracts_text_and_metadata() {
    let pdf_bytes = build_test_pdf(&["Hello PDF page 1", "Page 2 body"]);
    let mut tmp = tempfile::NamedTempFile::new().expect("tmp file");
    tmp.write_all(&pdf_bytes).expect("write pdf");
    let path = tmp.path().to_path_buf();

    let doc = load_pdf(&path).expect("load pdf");
    assert_eq!(doc.title.as_deref(), Some("Test Title"));
    assert_eq!(doc.author.as_deref(), Some("Test Author"));
    assert!(!doc.truncated);
    assert_eq!(doc.chapter_titles.len(), 2);

    let body: String = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            reader_core::types::Block::Paragraph(t) => Some(t.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("Hello PDF page 1"));
    assert!(body.contains("Page 2 body"));
    // Ensure page separator is present between pages
    assert!(body.contains("───"));
}

#[test]
fn pdf_loader_handles_page_limits() {
    let pdf_bytes = build_test_pdf(&["First page", "Second page", "Third page"]);
    let mut tmp = tempfile::NamedTempFile::new().expect("tmp file");
    tmp.write_all(&pdf_bytes).expect("write pdf");
    let path = tmp.path().to_path_buf();

    let doc = load_pdf_with_limit(&path, Some(2)).expect("load limited pdf");
    assert!(doc.truncated);
    assert_eq!(doc.chapter_titles.len(), 2);
    let text: String = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            reader_core::types::Block::Paragraph(t) => Some(t.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("First page"));
    assert!(text.contains("Second page"));
    assert!(text.contains("truncated: loaded 2 of 3 pages"));
}

#[test]
fn pdf_loader_loads_single_page_on_demand() {
    let pdf_bytes = build_test_pdf(&["Alpha", "Beta"]);
    let mut tmp = tempfile::NamedTempFile::new().expect("tmp file");
    tmp.write_all(&pdf_bytes).expect("write pdf");
    let path = tmp.path().to_path_buf();

    let mut loader = PdfLoader::open(&path).expect("open loader");
    assert_eq!(loader.page_count(), 2);
    let first = loader.load_page(0).expect("load page 0");
    assert!(first
        .iter()
        .any(|b| matches!(b, reader_core::types::Block::Paragraph(t) if t.contains("Alpha"))));
    // Loading the same page should hit the cache and still succeed
    let cached = loader.load_page(0).expect("load page 0 again");
    assert_eq!(first.len(), cached.len());
}

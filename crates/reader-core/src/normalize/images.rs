use super::inline::normalize_inline_text;

pub(crate) fn image_label_text(attrs: &kuchiki::Attributes) -> Option<String> {
    let label = attrs
        .get("alt")
        .or_else(|| attrs.get("title"))
        .or_else(|| attrs.get("aria-label"));
    label
        .map(normalize_inline_text)
        .filter(|label| !label.is_empty())
}

pub(crate) fn image_dimensions(attrs: &kuchiki::Attributes) -> (Option<u32>, Option<u32>) {
    (
        parse_dimension(attrs.get("width")),
        parse_dimension(attrs.get("height")),
    )
}

pub(crate) fn image_src(attrs: &kuchiki::Attributes) -> Option<String> {
    let raw = attrs
        .get("src")
        .or_else(|| attrs.get("data-src"))
        .or_else(|| attrs.get("data-original"))?;
    let mut s = raw.trim().to_string();
    if let Some(pos) = s.find('#') {
        s.truncate(pos);
    }
    if let Some(pos) = s.find('?') {
        s.truncate(pos);
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub(crate) fn image_fallback_text(
    alt: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
) -> String {
    if let Some(alt) = alt {
        let alt = alt.trim();
        if !alt.is_empty() {
            return format!("Image: {}", alt);
        }
    }
    match (width, height) {
        (Some(w), Some(h)) => format!("Image ({}x{})", w, h),
        _ => "Image".to_string(),
    }
}

fn parse_dimension(value: Option<&str>) -> Option<u32> {
    let value = value?;
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

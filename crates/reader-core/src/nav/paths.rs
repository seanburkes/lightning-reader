use std::path::{Path, PathBuf};

pub(crate) fn normalize_href_with_fragment(base_file: &Path, href: &str) -> String {
    let base_dir = base_file.parent().unwrap_or(Path::new(""));
    let (path_part, frag) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    let joined = if path_part.is_empty() {
        base_file.to_path_buf()
    } else {
        base_dir.join(path_part)
    };
    let mut out = normalize_path(&joined).to_string_lossy().to_string();
    if let Some(frag) = frag {
        if !frag.is_empty() {
            out.push('#');
            out.push_str(frag);
        }
    }
    out
}

pub(crate) fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            _ => out.push(comp.as_os_str()),
        }
    }
    out
}

use reader_core::types::Block as ReaderBlock;

use super::ReaderView;

#[cfg(feature = "kitty-images")]
use std::{env, io::Write};

#[cfg(feature = "kitty-images")]
use base64::Engine;
#[cfg(feature = "kitty-images")]
use crossterm::{cursor::MoveTo, queue};
#[cfg(feature = "kitty-images")]
use image::ImageFormat;
#[cfg(feature = "kitty-images")]
use ratatui::layout::Rect;

#[cfg(feature = "kitty-images")]
#[derive(Clone)]
pub(super) struct RenderImage {
    pub(super) id: String,
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

#[cfg(feature = "kitty-images")]
pub(super) struct KittyImage {
    pub(super) png_base64: String,
}

impl ReaderView {
    pub fn add_images_from_blocks(&mut self, blocks: &[ReaderBlock]) {
        for block in blocks {
            let ReaderBlock::Image(image) = block else {
                continue;
            };
            let Some(data) = image.data() else {
                continue;
            };
            if !self.image_map.contains_key(image.id()) {
                self.image_map.insert(image.id().to_string(), data.to_vec());
            }
        }
    }
}

#[cfg(feature = "kitty-images")]
impl ReaderView {
    pub(super) fn collect_image_placements(&mut self, page_idx: usize, area: Rect) {
        let Some(page) = self.pages.get(page_idx) else {
            return;
        };
        for (line_idx, line) in page.lines.iter().enumerate() {
            let Some(image) = &line.image else {
                continue;
            };
            let y = area.y.saturating_add(line_idx as u16);
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let cols = image.cols.min(area.width.max(1));
            self.image_placements.push(RenderImage {
                id: image.id.clone(),
                x: area.x,
                y,
                cols,
                rows: image.rows,
            });
        }
    }

    pub fn render_images<W: Write>(&mut self, out: &mut W) -> std::io::Result<()> {
        if self.image_placements.is_empty() || !kitty_supported() {
            return Ok(());
        }
        write!(out, "\x1b_Ga=d\x1b\\")?;
        let placements = self.image_placements.clone();
        for placement in placements {
            let Some(data) = self.image_map.get(&placement.id) else {
                continue;
            };
            let Some(encoded) = self.ensure_png_base64(&placement.id, data.as_slice()) else {
                continue;
            };
            queue!(out, MoveTo(placement.x, placement.y))?;
            send_kitty_image(out, &encoded, placement.cols.max(1), placement.rows.max(1))?;
        }
        out.flush()
    }

    fn ensure_png_base64(&mut self, id: &str, data: &[u8]) -> Option<String> {
        if let Some(cached) = self.image_cache.get(id) {
            return Some(cached.png_base64.clone());
        }
        let png = encode_png(data)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        self.image_cache.insert(
            id.to_string(),
            KittyImage {
                png_base64: encoded.clone(),
            },
        );
        Some(encoded)
    }
}

#[cfg(feature = "kitty-images")]
fn kitty_supported() -> bool {
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }
    env::var("TERM")
        .map(|term| term.contains("kitty"))
        .unwrap_or(false)
}

#[cfg(feature = "kitty-images")]
fn encode_png(data: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(data).ok()?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .ok()?;
    Some(out)
}

#[cfg(feature = "kitty-images")]
fn send_kitty_image<W: Write>(
    out: &mut W,
    base64: &str,
    cols: u16,
    rows: u16,
) -> std::io::Result<()> {
    let chunk_size = 4096usize;
    let bytes = base64.as_bytes();
    let total = (bytes.len() + chunk_size - 1) / chunk_size;
    for idx in 0..total {
        let start = idx * chunk_size;
        let end = (start + chunk_size).min(bytes.len());
        let chunk = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
        let last = idx + 1 == total;
        let mut params = String::new();
        if idx == 0 {
            params.push_str(&format!("a=T,f=100,C=1,c={},r={},q=2", cols, rows));
        }
        if !last {
            if !params.is_empty() {
                params.push(',');
            }
            params.push_str("m=1");
        }
        write!(out, "\x1b_G{};{}\x1b\\", params, chunk)?;
    }
    Ok(())
}

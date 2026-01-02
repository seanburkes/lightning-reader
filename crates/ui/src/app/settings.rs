use std::{fs, path::PathBuf};

use super::types::SpritzSettings;

fn settings_path() -> Option<PathBuf> {
    reader_core::config::config_root().map(|dir| dir.join("settings.toml"))
}

fn legacy_settings_paths() -> Vec<PathBuf> {
    reader_core::config::legacy_config_roots()
        .into_iter()
        .map(|dir| dir.join("settings.toml"))
        .collect()
}

pub(super) fn load_settings() -> (bool, bool, SpritzSettings) {
    let mut justify = false;
    let mut two_pane = false;
    let mut spritz = SpritzSettings::default();
    let mut candidates = Vec::new();
    if let Some(primary) = settings_path() {
        candidates.push(primary);
    }
    candidates.extend(legacy_settings_paths());
    for path in candidates {
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("justify=") {
                    justify = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("two_pane=") {
                    two_pane = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("spritz_wpm=") {
                    spritz.wpm = val.trim().parse().unwrap_or(spritz.wpm).clamp(100, 1000);
                } else if let Some(val) = line.strip_prefix("spritz_pause_on_punct=") {
                    spritz.pause_on_punct = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("spritz_punct_pause_ms=") {
                    spritz.punct_pause_ms = val.trim().parse().unwrap_or(spritz.punct_pause_ms);
                }
            }
            break;
        }
    }
    (justify, two_pane, spritz)
}

pub(super) fn save_settings(justify: bool, two_pane: bool, spritz: &SpritzSettings) {
    let target = settings_path().or_else(|| legacy_settings_paths().into_iter().next());
    if let Some(path) = target {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(
            path,
            format!(
                "justify={justify}\ntwo_pane={two_pane}\nspritz_wpm={}\nspritz_pause_on_punct={}\nspritz_punct_pause_ms={}\n",
                spritz.wpm, spritz.pause_on_punct, spritz.punct_pause_ms
            ),
        );
    }
}

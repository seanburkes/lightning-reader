use std::path::PathBuf;

use directories::{BaseDirs, ProjectDirs};

pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "sean";
pub const APPLICATION: &str = "librarian";

const LEGACY_QUALIFIER: &str = "org";
const LEGACY_APP_DIR: &str = "lightning-librarian";

pub fn config_root() -> Option<PathBuf> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).map(|p| p.config_dir().to_path_buf())
}

pub fn legacy_config_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = ProjectDirs::from(LEGACY_QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|p| p.config_dir().to_path_buf())
    {
        roots.push(dir);
    }
    if let Some(base) = BaseDirs::new().map(|b| b.config_dir().to_path_buf()) {
        roots.push(base.join(LEGACY_APP_DIR));
    }
    roots
}

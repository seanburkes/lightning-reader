mod archive;
mod epub2;
mod epub3;
mod paths;

pub use archive::{
    read_nav_entries, read_nav_entries_with_hints, read_nav_labels, read_nav_labels_with_hints,
};
pub(crate) use archive::{read_nav_entries_from_archive_inner, read_nav_labels_from_archive_inner};

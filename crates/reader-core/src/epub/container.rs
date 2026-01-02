use std::{fs::File, io::Read, path::PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use zip::ZipArchive;

use super::error::ReaderError;

pub(crate) fn read_container(zip: &mut ZipArchive<File>) -> Result<PathBuf, ReaderError> {
    let mut container = zip.by_name("META-INF/container.xml")?;
    let mut xml = String::new();
    container.read_to_string(&mut xml)?;
    let mut reader = XmlReader::from_str(&xml);
    let mut rootfile_path: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.contains("rootfile") {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        if key.contains("full-path") {
                            let val = a
                                .unescape_value()
                                .map_err(|e| ReaderError::Parse(e.to_string()))?;
                            rootfile_path = Some(val.into_owned());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ReaderError::Parse(e.to_string())),
            _ => {}
        }
    }
    let root = rootfile_path.ok_or_else(|| ReaderError::Parse("missing rootfile".into()))?;
    Ok(PathBuf::from(root))
}

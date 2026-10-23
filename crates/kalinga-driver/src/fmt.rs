//! `kalinga fmt` and `utkal fmt`: formatting files in place, or checking that they are.

use std::path::{Path, PathBuf};

use crate::error::{DriverError, Result, SourceMap};
use crate::fix::kl_files;

/// What `fmt_path` found.
#[derive(Debug, Default)]
pub struct FmtReport {
    /// Files that were not formatted (and now are, unless only checking).
    pub changed: Vec<PathBuf>,
    /// Files that could not be read as Kalinga (the errors say why).
    pub failed: Vec<DriverError>,
}

/// Formats `path` (a `.kl` file, or every `.kl` file under a folder); with `check`, only
/// reports the files that are not formatted.
pub fn fmt_path(path: &Path, check: bool) -> Result<FmtReport> {
    let mut report = FmtReport::default();
    for file in kl_files(path)? {
        let source = std::fs::read_to_string(&file).map_err(DriverError::io("read", &file))?;
        match kalinga_fmt::format(&source) {
            Ok(formatted) if formatted == source => {}
            Ok(formatted) => {
                if !check {
                    std::fs::write(&file, formatted).map_err(DriverError::io("write", &file))?;
                }
                report.changed.push(file);
            }
            Err(d) => {
                let sources = SourceMap { files: vec![(file.display().to_string(), source)], display: vec![] };
                report.failed.push(DriverError::compile(sources, d));
            }
        }
    }
    Ok(report)
}

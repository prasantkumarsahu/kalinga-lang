//! Kalinga's bundled standard modules (`import "std/..."`). They are compiled into the
//! `kalinga` executable and written to a folder on first use, so the compiler stays a
//! single file.

use std::path::PathBuf;

/// (file name inside `std/`, contents)
const FILES: &[(&str, &str)] = &[
    ("math2d.kl", include_str!("../std/math2d.kl")),
    ("game.kl", include_str!("../std/game.kl")),
    ("game_glue.rs", include_str!("../std/game_glue.rs")),
];

/// The folder holding this compiler version's std files, written if missing or stale.
pub fn dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("kalinga-build").join(format!("std-{}", env!("CARGO_PKG_VERSION")));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    for (name, text) in FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
        }
    }
    Ok(dir)
}

/// Is there a bundled module with this name (`"math2d"`)?
pub fn has(module: &str) -> bool {
    FILES.iter().any(|(n, _)| n.strip_suffix(".kl") == Some(module))
}

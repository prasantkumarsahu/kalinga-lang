// Rust side of `std/fs`. Errors are `cannot <action> `<path>`: <reason>`, with the reason
// worded the same on every system.
use std::io::ErrorKind;
use std::path::Path;

fn fail(action: &str, path: &str, e: std::io::Error) -> String {
    let reason = match e.kind() {
        ErrorKind::NotFound => "not found".to_string(),
        ErrorKind::PermissionDenied => "permission denied".to_string(),
        ErrorKind::AlreadyExists => "it already exists".to_string(),
        ErrorKind::InvalidData => "it is not text (UTF-8)".to_string(),
        ErrorKind::DirectoryNotEmpty => "the folder is not empty".to_string(),
        ErrorKind::IsADirectory => "it is a folder".to_string(),
        ErrorKind::NotADirectory => "it is not a folder".to_string(),
        _ => e.to_string(),
    };
    format!("cannot {} `{}`: {}", action, path, reason)
}

pub fn fs_read(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| fail("read", &path, e))
}

pub fn fs_write(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| fail("write", &path, e))
}

pub fn fs_append(path: String, text: String) -> Result<(), String> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| fail("append to", &path, e))
}

pub fn fs_delete_file(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| fail("delete", &path, e))
}

pub fn fs_copy(from: String, to: String) -> Result<(), String> {
    std::fs::copy(&from, &to).map(|_| ()).map_err(|e| fail("copy", &from, e))
}

pub fn fs_rename(from: String, to: String) -> Result<(), String> {
    std::fs::rename(&from, &to).map_err(|e| fail("rename", &from, e))
}

pub fn fs_is_file(path: String) -> bool {
    Path::new(&path).is_file()
}

pub fn fs_is_dir(path: String) -> bool {
    Path::new(&path).is_dir()
}

pub fn fs_list(path: String) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(&path).map_err(|e| fail("list", &path, e))?;
    let mut names = vec![];
    for entry in entries {
        let entry = entry.map_err(|e| fail("list", &path, e))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

pub fn fs_create_dir(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| fail("create", &path, e))
}

pub fn fs_delete_dir(path: String) -> Result<(), String> {
    std::fs::remove_dir(&path).map_err(|e| fail("delete", &path, e))
}

/// Splits `path` at its last `/` or `\` (ignoring trailing ones): (folder, last part).
fn split_last(path: &str) -> (&str, &str) {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
        None => ("", trimmed),
    }
}

pub fn fs_name(path: String) -> String {
    split_last(&path).1.to_string()
}

pub fn fs_parent(path: String) -> String {
    split_last(&path).0.to_string()
}

pub fn fs_extension(path: String) -> String {
    let name = split_last(&path).1;
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_string(),
        _ => String::new(),
    }
}

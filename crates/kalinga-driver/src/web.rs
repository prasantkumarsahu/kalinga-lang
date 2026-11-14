//! Web builds: the folder a browser loads (`index.html`, `app.js` and the styles), and
//! `kalinga serve`, which serves that folder on localhost and builds it again whenever the
//! code or the styles change (the open page then reloads itself).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{DriverError, Result, to_js};

/// What a web build of a program writes, and where.
pub struct WebPlan {
    /// The folder written: `dist` in the project, or `dist` next to the program.
    pub out: PathBuf,
    /// CSS files: (the file, its path inside `out`).
    pub styles: Vec<(PathBuf, String)>,
    /// The page's title: the package name, or the program's file name.
    pub title: String,
    /// The folder a rebuild watches: the project, or the program's folder.
    pub root: PathBuf,
}

/// Where a web build of `input` goes and which styles it takes. In a utkal project, the
/// build goes to the project's `dist` and takes the `styles` of `utkal.toml`; otherwise
/// (and when `styles` is not given) it takes every `.css` file next to `input`.
pub fn plan(input: &Path, out: Option<PathBuf>) -> Result<WebPlan> {
    let base = input.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    if let Some(project) = kalinga_project::find_project(input) {
        let manifest = kalinga_project::read_manifest(&project)?;
        let styles = match &manifest.styles {
            Some(list) => list.iter().map(|s| listed_style(&project, s)).collect::<Result<Vec<_>>>()?,
            None => css_next_to(&base),
        };
        return Ok(WebPlan {
            out: out.unwrap_or_else(|| project.join("dist")),
            styles,
            title: manifest.name,
            root: project,
        });
    }
    let title = input.file_stem().and_then(|s| s.to_str()).unwrap_or("app").to_string();
    Ok(WebPlan { out: out.unwrap_or_else(|| base.join("dist")), styles: css_next_to(&base), title, root: base })
}

/// A file named in `styles`, kept at its path inside the build (only its name when that
/// path leaves the project).
fn listed_style(project: &Path, listed: &str) -> Result<(PathBuf, String)> {
    let from = project.join(listed);
    if !from.is_file() {
        return Err(DriverError::MissingStyle { path: from });
    }
    let path = Path::new(listed);
    let inside = path.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let href = if inside {
        path.components()
            .filter_map(|c| match c {
                Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    } else {
        path.file_name().map_or_else(|| listed.to_string(), |n| n.to_string_lossy().into_owned())
    };
    Ok((from, href))
}

/// The `.css` files in `dir`, by name.
fn css_next_to(dir: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut css: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "css")).collect();
    css.sort();
    css.into_iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((p, name))
        })
        .collect()
}

/// Compiles `input` and writes the build `plan` describes; with `reload`, the page reloads
/// itself after `kalinga serve` builds it again.
pub fn write(input: &Path, plan: &WebPlan, reload: bool) -> Result<()> {
    let source = std::fs::read_to_string(input).map_err(DriverError::io("read", input))?;
    let js = to_js(&input.display().to_string(), &source, false)?;
    std::fs::create_dir_all(&plan.out).map_err(DriverError::io("create", &plan.out))?;
    let app = plan.out.join("app.js");
    std::fs::write(&app, js).map_err(DriverError::io("write", &app))?;
    for (from, href) in &plan.styles {
        let to = plan.out.join(href);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(DriverError::io("create", dir))?;
        }
        std::fs::copy(from, &to).map_err(DriverError::io("copy", from))?;
    }
    let hrefs: Vec<&str> = plan.styles.iter().map(|(_, h)| h.as_str()).collect();
    let index = plan.out.join("index.html");
    std::fs::write(&index, page(&plan.title, &hrefs, reload)).map_err(DriverError::io("write", &index))?;
    Ok(())
}

/// Asks the server for its build number twice a second, and reloads when it changes.
const RELOAD: &str = "  <script>
    (() => {
      let seen = null;
      setInterval(async () => {
        try {
          const now = await (await fetch(\"/__kalinga/version\")).text();
          if (seen !== null && now !== seen) location.reload();
          seen = now;
        } catch (e) {}
      }, 500);
    })();
  </script>
";

fn page(title: &str, styles: &[&str], reload: bool) -> String {
    let mut links = String::new();
    for s in styles {
        links.push_str(&format!("  <link rel=\"stylesheet\" href=\"{}\">\n", html_escape(s)));
    }
    format!(
        "<!doctype html>\n<html>\n<head>\n  <meta charset=\"utf-8\">\n  \
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  \
         <title>{}</title>\n{}</head>\n<body>\n  <div id=\"app\"></div>\n  \
         <script src=\"app.js\"></script>\n{}</body>\n</html>\n",
        html_escape(title),
        links,
        if reload { RELOAD } else { "" }
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// What the server shares between the rebuilds and the requests.
struct State {
    out: PathBuf,
    /// Goes up after every build, so open pages know to reload.
    version: AtomicU64,
    /// The errors of the last build, shown in the page until a build succeeds.
    error: Mutex<Option<String>>,
}

/// Builds `input`, serves the build at `http://localhost:<port>/`, and builds it again when
/// a `.kl` or `.css` file (or `utkal.toml`) under the project or program folder changes.
/// Runs until the process is stopped.
pub fn serve(input: &Path, out: Option<PathBuf>, port: u16) -> Result<()> {
    let plan = plan(input, out)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|source| DriverError::Port { port, source })?;
    let state = Arc::new(State { out: plan.out.clone(), version: AtomicU64::new(0), error: Mutex::new(None) });
    rebuild(input, &plan, &state);
    println!("serving {} at http://localhost:{}/ (Ctrl+C to stop)", plan.out.display(), port);
    let watcher = Arc::clone(&state);
    let input = input.to_path_buf();
    std::thread::spawn(move || {
        let mut last = fingerprint(&plan);
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let now = fingerprint(&plan);
            if now != last {
                last = now;
                rebuild(&input, &plan, &watcher);
            }
        }
    });
    for stream in listener.incoming().flatten() {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = handle(stream, &state);
        });
    }
    Ok(())
}

fn rebuild(input: &Path, plan: &WebPlan, state: &State) {
    let result = write(input, plan, true);
    let error = match result {
        Ok(()) => {
            println!("built {}", plan.out.join("index.html").display());
            None
        }
        Err(e) => {
            let text = e.to_string();
            eprintln!("{}", text);
            Some(text)
        }
    };
    if let Ok(mut slot) = state.error.lock() {
        *slot = error;
    }
    state.version.fetch_add(1, Ordering::SeqCst);
}

/// A number that changes when a file a build reads changes (its size or time).
fn fingerprint(plan: &WebPlan) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut files = vec![];
    watched(&plan.root, &plan.out, &mut files, 0);
    files.extend(plan.styles.iter().map(|(f, _)| f.clone()));
    files.sort();
    for f in files {
        if let Ok(meta) = std::fs::metadata(&f) {
            f.hash(&mut h);
            meta.len().hash(&mut h);
            meta.modified().ok().hash(&mut h);
        }
    }
    h.finish()
}

/// The `.kl`, `.css` and `utkal.toml` files under `dir`, leaving out the build folder,
/// `target` and hidden folders.
fn watched(dir: &Path, out: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            let skip = name.starts_with('.') || name == "target" || name == "node_modules" || same_dir(&path, out);
            if !skip && depth < 8 {
                watched(&path, out, files, depth + 1);
            }
        } else if name == "utkal.toml" || path.extension().is_some_and(|x| x == "kl" || x == "css") {
            files.push(path);
        }
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn handle(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let error = state.error.lock().ok().and_then(|e| e.clone());
    let r = respond(method, target, &state.out, state.version.load(Ordering::SeqCst), error.as_deref());
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        r.status,
        r.kind,
        r.body.len()
    )?;
    if method != "HEAD" {
        stream.write_all(&r.body)?;
    }
    stream.flush()
}

/// An answer to a request.
struct Response {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
}

/// The answer to `method target` from the build folder `out`. Paths without a file
/// extension that are not files get the page, so an app's own addresses (`History.push`)
/// still load it; while the last build failed, the page shows its errors.
fn respond(method: &str, target: &str, out: &Path, version: u64, error: Option<&str>) -> Response {
    let text =
        |status, body: &str| Response { status, kind: "text/plain; charset=utf-8", body: body.as_bytes().to_vec() };
    if method != "GET" && method != "HEAD" {
        return text("405 Method Not Allowed", "only GET and HEAD are served");
    }
    let path = target.split(['?', '#']).next().unwrap_or("/");
    if path == "/__kalinga/version" {
        return text("200 OK", &version.to_string());
    }
    let Some(rel) = safe_path(path) else { return text("400 Bad Request", "bad path") };
    let is_page = rel.extension().is_none() || rel.extension().is_some_and(|x| x == "html");
    if let Some(e) = error.filter(|_| is_page) {
        let body = format!(
            "<!doctype html>\n<html>\n<head>\n  <meta charset=\"utf-8\">\n  <title>Build failed</title>\n</head>\n\
             <body style=\"font-family: monospace; margin: 2rem\">\n  <h2>Build failed</h2>\n  <pre>{}</pre>\n{}</body>\n</html>\n",
            html_escape(e),
            RELOAD
        );
        return Response { status: "200 OK", kind: "text/html; charset=utf-8", body: body.into_bytes() };
    }
    let file = out.join(&rel);
    let file = if file.is_dir() { file.join("index.html") } else { file };
    let file = if !file.is_file() && rel.extension().is_none() { out.join("index.html") } else { file };
    match std::fs::read(&file) {
        Ok(body) => Response { status: "200 OK", kind: content_type(&file), body },
        Err(_) => text("404 Not Found", &format!("{} not found", path)),
    }
}

/// The path inside the build folder a URL path names, or `None` when it tries to leave it.
fn safe_path(path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    let mut rel = PathBuf::new();
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(['\\', ':']) => return None,
            p => rel.push(p),
        }
    }
    Some(rel)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(file: &Path) -> &'static str {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_build() {
        assert_eq!(safe_path("/"), Some(PathBuf::new()));
        assert_eq!(safe_path("/styles/main.css"), Some(PathBuf::from("styles").join("main.css")));
        assert_eq!(safe_path("/my%20file.css"), Some(PathBuf::from("my file.css")));
        assert_eq!(safe_path("/../secret"), None);
        assert_eq!(safe_path("/a/%2e%2e/%2e%2e/x"), None);
        assert_eq!(safe_path("/C:/Windows"), None);
    }

    #[test]
    fn serves_files_and_falls_back_to_the_page() {
        let out = std::env::temp_dir().join(format!("kalinga-serve-test-{}", std::process::id()));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("index.html"), "<p>page</p>").unwrap();
        std::fs::write(out.join("app.js"), "1;").unwrap();
        let get = |t: &str| respond("GET", t, &out, 3, None);
        assert_eq!(get("/").body, b"<p>page</p>");
        assert_eq!(get("/app.js?v=2").kind, "text/javascript; charset=utf-8");
        assert_eq!(get("/items/3").body, b"<p>page</p>", "the app's own addresses load the page");
        assert_eq!(get("/missing.png").status, "404 Not Found");
        assert_eq!(get("/__kalinga/version").body, b"3");
        assert_eq!(respond("POST", "/", &out, 0, None).status, "405 Method Not Allowed");
        let failed = respond("GET", "/", &out, 0, Some("main.kl:1:1: error: <oops>"));
        assert!(String::from_utf8(failed.body).unwrap().contains("&lt;oops&gt;"));
        assert_eq!(respond("GET", "/app.js", &out, 0, Some("e")).body, b"1;");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn listed_styles_keep_their_place() {
        let dir = std::env::temp_dir().join(format!("kalinga-styles-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("styles")).unwrap();
        std::fs::write(dir.join("styles").join("main.css"), "").unwrap();
        assert_eq!(listed_style(&dir, "styles/main.css").unwrap().1, "styles/main.css");
        assert_eq!(listed_style(&dir, "./styles/main.css").unwrap().1, "styles/main.css");
        assert!(matches!(listed_style(&dir, "nope.css"), Err(DriverError::MissingStyle { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

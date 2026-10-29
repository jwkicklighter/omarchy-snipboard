//! Persistent JSON-lines worker. Search never reads disk or launches processes.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use unicode_segmentation::UnicodeSegmentation;

use crate::core::{MAX_KEYWORD, Rendered, char_len, render, scratch_match};
use crate::history::History;
use crate::json::{replace_lone_surrogates, to_ascii_string};
use crate::search::{Index, Kind, RESULT_LIMIT, Record};
use crate::storage::{Library, Snippet, Store, Usage, UsageEntry, read_library, read_usage, validate_library, write_usage};
use crate::{Error, Result, source_roots, sys};

pub const MAX_LINE: u64 = 512 * 1024;
pub const FATAL: &str = "Prototype storage unavailable or invalid; existing files preserved.";
const GENERIC: &str = "Action failed; check prototype storage or capability. Existing data was not reset.";
const INVALID_SNIPPET: &str = "Invalid snippet: use a nonblank name (256 max), body ≤65,536, notes ≤4,096, and an optional whitespace-free keyword ≤64. Keywords must not duplicate or prefix another; use at most one {cursor}.";
const PASTE_LUA: &str = include_str!("paste.lua");

/// Only fixed, payload-free messages may cross the UI boundary.
#[derive(Debug)]
pub enum Failure {
    /// A library action failed; the response also carries the library state.
    Library(&'static str),
    Action(&'static str),
    Generic,
}

impl From<Error> for Failure {
    fn from(_: Error) -> Self {
        Failure::Generic
    }
}

type Response = std::result::Result<Map<String, Value>, Failure>;

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => unreachable!("responses are objects"),
    }
}

/// A string field that defaults when absent but must be a string when present.
fn text<'a>(request: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    match request.get(key) {
        None => Some(""),
        Some(value) => value.as_str(),
    }
}

/// Resolve an absolute path's existing prefix through symlinks, keeping the remainder.
fn resolve(path: &Path) -> PathBuf {
    let mut remainder = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(resolved) = existing.canonicalize() {
            return remainder.iter().rev().fold(resolved, |path, part| path.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                remainder.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

pub struct Worker {
    pub store: Store,
    _lock: File,
    pub usage: Usage,
    pub index: Index,
    history: History,
    child: Option<Child>,
}

impl Worker {
    pub fn new(root: &Path) -> Result<Self> {
        Self::with_library_root(root, root)
    }

    pub fn with_library_root(root: &Path, library_root: &Path) -> Result<Self> {
        if !root.is_absolute() || root.components().any(|c| c == Component::ParentDir) {
            return Err(Error::Invalid("prototype data must be outside source at an absolute path"));
        }
        let resolved = resolve(root);
        if source_roots().iter().any(|source| resolved.starts_with(source)) {
            return Err(Error::Invalid("prototype data must be outside source at an absolute path"));
        }
        let mut store = Store::with_library_root(root, library_root)?;
        let lock = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(store.root.join("worker.lock"))?;
        lock.try_lock().map_err(|error| Error::Io(error.into()))?;
        store.migrate_library()?;
        let library = store.initialize(&Library::default())?.clone();
        let usage = if store.usage_path.exists() { read_usage(&store.usage_path)? } else { Usage::new() };
        let history = History::read(&store.root.join("history.local.json"), sys::now())?;
        let mut records = history.records();
        records.extend(library.snippets.iter().map(Record::from));
        let index = Index::new(records, &usage);
        Ok(Worker { store, _lock: lock, usage, index, history, child: None })
    }

    /// Stop the clipboard process this worker owns, if any. Never touches other processes.
    pub fn cleanup(&mut self) {
        let Some(mut child) = self.child.take() else { return };
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        // SAFETY: signalling our own unreaped child; its PID cannot have been reused.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    pub fn library_state(&self) -> Map<String, Value> {
        object(json!({"revision": self.store.revision(), "readonly": self.store.readonly}))
    }

    fn snippet(&self, record: &str) -> std::result::Result<&Record, Failure> {
        self.index
            .get(record)
            .filter(|item| item.kind == Kind::Snippet)
            .ok_or(Failure::Library("Snippet no longer exists. Reopen the editor."))
    }

    fn record(&self, request: &Map<String, Value>) -> std::result::Result<&Record, Failure> {
        request
            .get("record")
            .and_then(Value::as_str)
            .and_then(|id| self.index.get(id))
            .ok_or(Failure::Generic)
    }

    pub fn prepare_library(&self, library: &Library) -> Index {
        let mut records: Vec<Record> =
            self.index.records().iter().filter(|r| r.kind == Kind::Clipboard).cloned().collect();
        records.extend(library.snippets.iter().map(Record::from));
        Index::new(records, &self.usage)
    }

    fn change_library(&mut self, request: &Map<String, Value>, delete: bool) -> Response {
        if self.store.readonly {
            return Err(Failure::Library("Library is read-only. Review storage and explicitly reload before saving."));
        }
        if request.get("revision").and_then(Value::as_str) != Some(self.store.revision().as_str()) {
            return Err(Failure::Library("Library changed since this editor opened. Keep your draft and reopen before saving."));
        }
        let mut record = text(request, "record").ok_or(Failure::Library("Invalid snippet identity."))?.to_owned();
        if !record.is_empty() {
            self.snippet(&record)?;
        }
        let mut library = self.store.library.clone();
        if delete {
            if record.is_empty() {
                return Err(Failure::Library("Select a saved snippet to delete."));
            }
            library.snippets.retain(|s| s.id != record);
        } else {
            let fields = request
                .get("fields")
                .and_then(Value::as_object)
                .filter(|f| f.len() == 4 && ["name", "body", "notes", "keyword"].iter().all(|k| f.contains_key(*k)))
                .ok_or(Failure::Library("Name, body, notes, and keyword are required."))?;
            let field = |key: &str| fields[key].as_str().map(str::to_owned).ok_or(Failure::Library(INVALID_SNIPPET));
            if record.is_empty() {
                record = format!("snippet-{}", sys::random_hex(16).map_err(|_| Failure::Generic)?);
            }
            let item = Snippet {
                id: record.clone(),
                name: field("name")?,
                keyword: field("keyword")?,
                body: field("body")?,
                notes: field("notes")?,
            };
            match library.snippets.iter_mut().find(|s| s.id == record) {
                Some(existing) => *existing = item,
                None => library.snippets.push(item),
            }
        }
        validate_library(&library).map_err(|_| Failure::Library(INVALID_SNIPPET))?;
        // Prepare first, persist second, publish last: a failed save never changes search.
        let fresh = self.prepare_library(&library);
        if self.store.save(library).is_err() {
            return Err(Failure::Library(
                "Save could not complete. Draft retained; review storage or external changes before reloading.",
            ));
        }
        self.index = fresh;
        let mut response = self.library_state();
        response.insert("record".into(), record.into());
        response.insert("deleted".into(), delete.into());
        Ok(response)
    }

    fn reload_library(&mut self) -> Response {
        let Ok(library) = read_library(&self.store.library_path) else {
            self.store.readonly = true;
            return Err(Failure::Library(
                "Reload failed. Last valid library is available read-only; existing files were preserved.",
            ));
        };
        self.index = self.prepare_library(&library);
        self.store.library = library;
        self.store.readonly = false;
        Ok(self.library_state())
    }

    fn copy(&mut self, text: &str) -> Result<()> {
        self.cleanup();
        let mut child = Command::new("setpriv")
            .args(["--pdeathsig", "TERM", "wl-copy", "--foreground", "--type", "text/plain;charset=utf-8"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let written = child.stdin.take().map_or(Ok(()), |mut stdin| stdin.write_all(text.as_bytes()));
        self.child = Some(child);
        Ok(written?)
    }

    fn paste(&mut self, result: &Rendered) -> Response {
        self.cleanup();
        paste_text(&result.text, cursor_steps(result), Command::new("wl-copy"), Command::new("hyprctl"))?;
        Ok(object(json!({"ok": true})))
    }

    fn rendered(&self, request: &Map<String, Value>) -> std::result::Result<Rendered, Failure> {
        let item = self.record(request)?;
        if item.kind == Kind::Clipboard {
            // Clipboard text is never a template, including escaped variable syntax.
            Ok(Rendered { text: item.body.clone(), cursor: None })
        } else {
            render_template(&item.body, request)
        }
    }

    pub fn dispatch(&mut self, request: &Map<String, Value>) -> Response {
        let operation = request.get("op").and_then(Value::as_str).unwrap_or_default();
        match operation {
            "clipboard-capture" => {
                let body = text(request, "text").ok_or(Failure::Generic)?;
                let record = self.history.capture(&self.store.root.join("history.local.json"), body, sys::now())?;
                let mut records = self.history.records();
                records.extend(self.store.library.snippets.iter().map(Record::from));
                self.index = Index::new(records, &self.usage);
                Ok(object(json!({"record": record})))
            }
            "clipboard-delete" => {
                let item = self.record(request)?;
                if item.kind != Kind::Clipboard {
                    return Err(Failure::Action("Select a clipboard history entry to delete."));
                }
                let record = item.id.clone();
                self.history.delete(&self.store.root.join("history.local.json"), &record)?;
                let mut records = self.history.records();
                records.extend(self.store.library.snippets.iter().map(Record::from));
                self.index = Index::new(records, &self.usage);
                Ok(object(json!({"record": record, "deleted": true})))
            }
            "snippet-get" => {
                let record = text(request, "record").ok_or(Failure::Library("Invalid snippet identity."))?;
                let snippet = if record.is_empty() {
                    json!({"id": "", "name": "", "body": "", "notes": "", "keyword": ""})
                } else {
                    let item = self.snippet(record)?;
                    json!({"id": item.id, "name": item.name, "body": item.body, "notes": item.notes, "keyword": item.keyword})
                };
                let mut response = self.library_state();
                response.insert("snippet".into(), snippet);
                Ok(response)
            }
            "snippet-save" => self.change_library(request, false),
            "snippet-draft" => {
                let item = self.record(request)?;
                let (name, body, notes) = if item.kind == Kind::Snippet {
                    // A duplicate keeps the template, but never shares identity,
                    // expansion keyword, or usage with its source.
                    let name: String = item.name.chars().take(249).collect();
                    (format!("{name} (copy)"), item.body.clone(), item.notes.clone())
                } else {
                    (item.name.chars().take(256).collect(), crate::core::literal_template(&item.body), String::new())
                };
                if char_len(&body) > crate::core::MAX_BODY || body.contains('\0') {
                    return Err(Failure::Library("This text cannot fit in a snippet without alteration. Source preserved."));
                }
                let mut response = self.library_state();
                response.insert("snippet".into(), json!({"id": "", "name": name, "body": body, "notes": notes, "keyword": ""}));
                Ok(response)
            }
            "snippet-delete" => self.change_library(request, true),
            "library-reload" => self.reload_library(),
            "search" => {
                let query = text(request, "query").ok_or(Failure::Generic)?;
                let kind = request.get("kind").map_or(Some("all"), Value::as_str).ok_or(Failure::Generic)?;
                let start = Instant::now();
                let rows = self.index.search(query, kind, RESULT_LIMIT, sys::now())?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let rows: Vec<Value> = rows
                    .into_iter()
                    .map(|r| {
                        let summary: String = r.body.chars().take(100).collect();
                        json!({"id": r.id, "name": r.name, "kind": r.kind.as_str(), "summary": summary.replace('\n', " ↵ ")})
                    })
                    .collect();
                Ok(object(json!({"rows": rows, "matchMs": elapsed})))
            }
            "preview" => {
                let item = self.record(request)?;
                Ok(object(json!({"body": item.body, "keyword": item.keyword, "kind": item.kind.as_str(), "created": item.created})))
            }
            "scratch" => {
                if request.get("enabled") != Some(&Value::Bool(true))
                    || request.get("context").and_then(Value::as_str) != Some("trusted-scratch")
                {
                    return Ok(object(json!({"expansion": null})));
                }
                let prefix = text(request, "prefix").ok_or(Failure::Generic)?;
                if char_len(prefix) > MAX_KEYWORD + 1 {
                    return Err(Failure::Generic);
                }
                let Some(snippet) = scratch_match(prefix, &self.store.library.snippets) else {
                    return Ok(object(json!({"expansion": null})));
                };
                let clipboard = text(request, "clipboard").ok_or(Failure::Generic)?;
                let result = render(&snippet.body, clipboard, None)?;
                Ok(object(json!({"expansion": {
                    "record": snippet.id,
                    "remove": crate::core::utf16_length(&snippet.keyword),
                    "cursor": result.utf16_cursor(),
                    "text": result.text,
                }})))
            }
            "copy" => {
                let result = self.rendered(request)?;
                self.copy(&result.text)?;
                Ok(object(json!({"message": "Copy requested (clipboard ownership unacknowledged).", "confirmed": false})))
            }
            "paste" => {
                let item = self.record(request)?;
                let record = item.id.clone();
                let result = self.rendered(request)?;
                let mut response = self.paste(&result)?;
                response.insert("record".into(), record.into());
                Ok(response)
            }
            "used" => {
                let item = self.record(request)?;
                if item.kind == Kind::Snippet {
                    let id = item.id.clone();
                    let mut fresh = self.usage.clone();
                    let previous = fresh.get(&id).map_or(0, |entry| entry.count);
                    fresh.insert(id, UsageEntry { count: previous + 1, last: sys::now() });
                    write_usage(&self.store.usage_path, &fresh)?;
                    self.usage = fresh;
                    self.index.update_usage(&self.usage);
                }
                Ok(object(json!({"ok": true})))
            }
            "render" => {
                let result = self.rendered(request)?;
                Ok(object(json!({"cursor": result.utf16_cursor(), "text": result.text})))
            }
            "shutdown" => {
                self.cleanup();
                Ok(object(json!({"ok": true})))
            }
            _ => Err(Failure::Generic),
        }
    }

    /// Handle one protocol line, returning the response object.
    pub fn handle(&mut self, line: &[u8]) -> (Map<String, Value>, bool) {
        let request = match serde_json::from_slice::<Value>(&replace_lone_surrogates(line)) {
            Ok(Value::Object(request)) => request,
            _ => Map::new(),
        };
        let mut response = match self.dispatch(&request) {
            Ok(response) => response,
            Err(Failure::Library(message)) => {
                let mut response = self.library_state();
                response.insert("error".into(), message.into());
                response
            }
            Err(Failure::Generic) => object(json!({"error": GENERIC})),
            Err(Failure::Action(message)) => object(json!({"error": message})),
        };
        response.insert("op".into(), request.get("op").cloned().unwrap_or(Value::Null));
        response.insert("seq".into(), request.get("seq").cloned().unwrap_or(json!(0)));
        let shutdown = request.get("op").and_then(Value::as_str) == Some("shutdown");
        (response, shutdown)
    }
}

// Explicit clipboard values are reserved for deterministic scratchpad/protocol tests.
// Ordinary copy/paste reads the selection before wl-copy replaces it.
fn render_template(body: &str, request: &Map<String, Value>) -> std::result::Result<Rendered, Failure> {
    let clipboard = match request.get("clipboard") {
        Some(value) => value.as_str().ok_or(Failure::Generic)?.to_owned(),
        None if crate::core::tokens(body).any(|t| !t.escaped && t.variable == crate::core::Variable::Clipboard) => {
            let mut command = Command::new("timeout");
            command.args(["2", "wl-paste", "--no-newline", "--type", "text"]);
            read_clipboard(command)?
        }
        None => String::new(),
    };
    Ok(render(body, &clipboard, None)?)
}

fn read_clipboard(mut command: Command) -> std::result::Result<String, Failure> {
    const FAILED: Failure = Failure::Action("Could not read clipboard text. Copy some text and try again.");
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|_| FAILED)?;
    let mut bytes = Vec::new();
    let read = child.stdout.take().unwrap().take((crate::core::MAX_CLIPBOARD * 4 + 1) as u64).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > crate::core::MAX_CLIPBOARD * 4 {
        let _ = child.kill();
        let _ = child.wait();
        return Err(FAILED);
    }
    if !child.wait().is_ok_and(|status| status.success()) {
        return Err(FAILED);
    }
    let value = String::from_utf8(bytes).map_err(|_| FAILED)?;
    if value.contains('\0') || char_len(&value) > crate::core::MAX_CLIPBOARD {
        return Err(FAILED);
    }
    Ok(value)
}

fn cursor_steps(result: &Rendered) -> usize {
    result.cursor.map_or(0, |cursor| {
        let suffix: String = result.text.chars().skip(cursor).collect();
        suffix.graphemes(true).count()
    })
}

fn paste_script(steps: usize) -> String {
    // Only a bounded integer enters Lua, never snippet or clipboard content.
    PASTE_LUA.replace("local cursor_steps = 0", &format!("local cursor_steps = {steps}"))
}

fn paste_text(text: &str, steps: usize, mut copy: Command, mut input: Command) -> Response {
    const COPY_FAILED: &str = "Paste failed: could not set the clipboard. Check wl-copy and the Wayland session, or use Copy only.";
    const INPUT_FAILED: &str = "Text copied, but the paste shortcut failed. Check the Hyprland session or paste manually.";
    // The default wl-copy parent exits only after setting the selection. Its
    // background owner must remain alive to serve the target's clipboard request.
    // Send text on stdin, never through shell interpolation or process arguments.
    let mut child = copy.args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|_| Failure::Action(COPY_FAILED))?;
    let written = child.stdin.take().unwrap().write_all(text.as_bytes());
    let copied = child.wait();
    if written.is_err() || !copied.is_ok_and(|status| status.success()) {
        return Err(Failure::Action(COPY_FAILED));
    }
    // The script is fixed code, never snippet text. Select the chord inside the
    // compositor using Omarchy's terminal tags and explicitly control modifiers.
    let output = input.args(["eval", &paste_script(steps)])
        .stdin(Stdio::null()).stderr(Stdio::null())
        .output().map_err(|_| Failure::Action(INPUT_FAILED))?;
    // hyprctl can report a Lua/IPC error on stdout even with exit status zero.
    if !output.status.success() || output.stdout != b"ok\n" && output.stdout != b"ok" {
        return Err(Failure::Action(INPUT_FAILED));
    }
    Ok(object(json!({"ok": true})))
}

/// Block termination signals here and handle them on a dedicated thread, which waits
/// for any in-flight request (such as an atomic write) before cleaning up and exiting.
fn handle_signals(worker: Arc<Mutex<Worker>>) {
    // SAFETY: sigset manipulation on a local set; the mask is inherited by the thread
    // spawned below. Child processes get a reset mask from std::process::Command.
    let signals = unsafe {
        let mut signals: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut signals);
        libc::sigaddset(&mut signals, libc::SIGTERM);
        libc::sigaddset(&mut signals, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &signals, std::ptr::null_mut());
        signals
    };
    thread::spawn(move || {
        let mut received = 0;
        // SAFETY: waits on the blocked set built above.
        while unsafe { libc::sigwait(&signals, &mut received) } != 0 {}
        let mut worker = worker.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        worker.cleanup();
        std::process::exit(0);
    });
}

pub fn serve(root: &Path) -> Result<()> {
    serve_with_library_root(root, root)
}

pub fn serve_with_library_root(root: &Path, library_root: &Path) -> Result<()> {
    let worker = Arc::new(Mutex::new(Worker::with_library_root(root, library_root)?));
    handle_signals(Arc::clone(&worker));
    let mut stdout = io::stdout().lock();
    let ready = {
        let worker = worker.lock().unwrap();
        let mut ready = worker.library_state();
        ready.insert("op".into(), "ready".into());
        ready.insert("count".into(), worker.index.records().len().into());
        ready
    };
    writeln!(stdout, "{}", to_ascii_string(&ready))?;
    stdout.flush()?;

    let mut stdin = io::stdin().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        io::Read::take(&mut stdin, MAX_LINE + 1).read_until(b'\n', &mut line)?;
        if line.is_empty() || line.len() as u64 > MAX_LINE || line.last() != Some(&b'\n') {
            break;
        }
        // Hold the worker only while handling, never while blocked on I/O.
        let (response, shutdown) = worker.lock().unwrap().handle(&line);
        writeln!(stdout, "{}", to_ascii_string(&response))?;
        stdout.flush()?;
        if shutdown {
            break;
        }
    }
    worker.lock().unwrap().cleanup();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FAIL_REPLACE;
    use std::fs;

    fn worker() -> (tempfile::TempDir, Worker) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let mut worker = Worker::new(directory.path()).unwrap();
        worker.store.save(crate::core::synthetic_library(1000)).unwrap();
        worker.index = Index::new(crate::core::synthetic_records(&worker.store.library, 10000), &worker.usage);
        (directory, worker)
    }

    fn request(worker: &mut Worker, value: Value) -> Map<String, Value> {
        worker.handle(to_ascii_string(&value).as_bytes()).0
    }

    #[test]
    fn fresh_install_starts_empty_and_preserves_saved_snippets_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("data");
        let mut worker = Worker::new(&root).unwrap();
        assert_eq!(worker.store.library, Library::default());
        assert_eq!(read_library(&worker.store.library_path).unwrap(), Library::default());
        assert!(worker.index.records().is_empty());
        drop(worker);

        worker = Worker::new(&root).unwrap();
        assert!(worker.store.library.snippets.is_empty());
        assert!(worker.index.records().is_empty());
        let library = crate::core::synthetic_library(4);
        worker.store.save(library.clone()).unwrap();
        drop(worker);

        let worker = Worker::new(&root).unwrap();
        assert_eq!(worker.store.library, library);
        assert_eq!(worker.index.records().len(), 4);
    }

    #[test]
    fn rejects_relative_or_source_data_paths() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        for path in [source.join("forbidden-data"), PathBuf::from("relative"), PathBuf::from("/tmp/../tmp/x")] {
            assert!(matches!(Worker::new(&path), Err(Error::Invalid(_))), "{path:?}");
        }
        assert!(!source.join("forbidden-data").exists());
    }

    #[test]
    fn failed_persistence_keeps_live_index() {
        let (directory, mut worker) = worker();
        let before = fs::read(&worker.store.library_path).unwrap();
        let revision = worker.store.revision();
        let fields = json!({"name": "Unsaved fixture", "body": "exact", "notes": "", "keyword": ""});
        FAIL_REPLACE.set(true);
        let response = request(&mut worker, json!({"op": "snippet-save", "record": "", "revision": revision, "fields": fields}));
        FAIL_REPLACE.set(false);
        assert!(response["error"].as_str().unwrap().starts_with("Save could not complete"));
        assert_eq!(response["readonly"], true);
        assert_eq!(fs::read(&worker.store.library_path).unwrap(), before);
        assert!(worker.index.search("Unsaved fixture", "all", 60, 0.0).unwrap().is_empty());
        assert_eq!(worker.index.records().len(), 11000);
        let leftovers = fs::read_dir(directory.path())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(".snipboard-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn failed_history_delete_preserves_live_index() {
        let (_directory, mut worker) = worker();
        let captured = request(&mut worker, json!({"op": "clipboard-capture", "text": "Keep after failed delete"}));
        let path = worker.store.root.join("history.local.json");
        let before = fs::read(&path).unwrap();
        let records = worker.index.records().to_vec();
        FAIL_REPLACE.set(true);
        let response = request(&mut worker, json!({"op": "clipboard-delete", "seq": 42, "record": captured["record"]}));
        FAIL_REPLACE.set(false);
        assert!(response.contains_key("error"));
        assert_eq!(response["seq"], 42);
        assert_eq!(worker.index.records(), records);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn draft_limits_do_not_truncate_content_or_split_unicode_names() {
        let (_directory, mut worker) = worker();
        let mut library = worker.store.library.clone();
        library.snippets[0].name = "🌍".repeat(256);
        library.snippets[0].body = "\t🌍\r\n{date}{cursor}".into();
        library.snippets[0].notes = "Exact\r\nnotes".into();
        worker.index = worker.prepare_library(&library);
        let response = request(&mut worker, json!({"op": "snippet-draft", "record": "snippet-0000"}));
        assert_eq!(char_len(response["snippet"]["name"].as_str().unwrap()), 256);
        assert_eq!(response["snippet"]["body"], library.snippets[0].body);
        assert_eq!(response["snippet"]["notes"], library.snippets[0].notes);
        let mut records = worker.index.records().to_vec();
        let item = records.iter_mut().find(|r| r.id == "clipboard-00001").unwrap();
        let source = "\t🌍\r\n{date} {{time}} {clipboard} {cursor}{cursor} {unknown}\n ";
        item.body = source.into();
        worker.index = Index::new(records, &worker.usage);
        let response = request(&mut worker, json!({"op": "snippet-draft", "record": "clipboard-00001"}));
        let body = response["snippet"]["body"].as_str().unwrap();
        assert_eq!(render(body, "unused", None).unwrap().text, source);
        let mut records = worker.index.records().to_vec();
        let item = records.iter_mut().find(|r| r.id == "clipboard-00001").unwrap();
        item.body = "{date}".repeat(crate::core::MAX_BODY / 6);
        worker.index = Index::new(records, &worker.usage);
        let response = request(&mut worker, json!({"op": "snippet-draft", "record": "clipboard-00001"}));
        assert!(response["error"].as_str().unwrap().starts_with("This text cannot fit"));
    }

    #[test]
    fn errors_echo_op_and_seq() {
        let (_directory, mut worker) = worker();
        let unknown = request(&mut worker, json!({"op": "unknown", "seq": 7}));
        assert_eq!((unknown["op"].as_str(), unknown["seq"].as_i64()), (Some("unknown"), Some(7)));
        assert!(unknown.contains_key("error") && !unknown.contains_key("readonly"));
        let (malformed, _) = worker.handle(b"[1]\n");
        assert_eq!((malformed["op"].clone(), malformed["seq"].clone()), (Value::Null, json!(0)));
        // A lone surrogate from the UI is rejected as invalid input, not as an unparseable line.
        let state = worker.library_state();
        let line = format!(
            r#"{{"op":"snippet-save","seq":9,"record":"snippet-0000","revision":"{}","fields":{{"name":"\ud800","body":"","notes":"","keyword":""}}}}"#,
            state["revision"].as_str().unwrap()
        );
        let (rejected, _) = worker.handle(line.as_bytes());
        assert_eq!(rejected["seq"], 9);
        assert_eq!(rejected["error"], INVALID_SNIPPET);
        assert_eq!(rejected["readonly"], false);
    }

    #[test]
    fn cleanup_stops_only_the_owned_child() {
        let (_directory, mut worker) = worker();
        worker.child = Some(Command::new("sleep").arg("30").spawn().unwrap());
        let started = Instant::now();
        worker.cleanup();
        assert!(worker.child.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        worker.cleanup();
    }

    #[test]
    fn clipboard_reader_preserves_text_and_rejects_failures() {
        let command = |script: &str| {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        };
        let value = read_clipboard(command("printf '🌍\\n{cursor}\\n\\n'")).unwrap();
        assert_eq!(value, "🌍\n{cursor}\n\n");
        let request = object(json!({"clipboard": value}));
        let rendered = render_template("[{clipboard}]{cursor}end", &request).unwrap();
        assert_eq!(rendered.text, "[🌍\n{cursor}\n\n]end");
        assert_eq!(cursor_steps(&rendered), 3);
        assert!(read_clipboard(command("exit 1")).is_err());
        assert!(read_clipboard(command("printf '\\000'")).is_err());
        assert!(read_clipboard(command("printf '\\377'")).is_err());
        assert!(read_clipboard(command("head -c 65537 /dev/zero")).is_err());
        // No clipboard capability is needed without an active placeholder.
        assert_eq!(render_template("{{clipboard}}", &Map::new()).unwrap().text, "{clipboard}");
    }

    #[test]
    fn cursor_navigation_counts_graphemes_not_utf16_units() {
        for (body, steps) in [
            ("before{cursor}🌍e\u{301}\r\n👨‍👩‍👧‍👦", 4),
            ("{cursor}text", 4),
            ("text{cursor}", 0),
            ("text", 0),
        ] {
            let rendered = render(body, "", None).unwrap();
            assert_eq!(cursor_steps(&rendered), steps);
        }
        assert!(paste_script(4).contains("local cursor_steps = 4"));
    }

    #[test]
    fn paste_copies_exact_bytes_before_shortcut_and_reports_failures() {
        for (copy_exit, input_exit, reply) in [(0, 0, "ok"), (7, 0, "ok"), (0, 8, "ok"), (0, 0, "Lua error")] {
            let directory = tempfile::tempdir().unwrap();
            let fixture = |name: &str| {
                let mut command = Command::new("/bin/sh");
                command.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name))
                    .env("SNIPBOARD_TEST_OUTPUT", directory.path())
                    .env("SNIPBOARD_TEST_COPY_EXIT", copy_exit.to_string())
                    .env("SNIPBOARD_TEST_INPUT_EXIT", input_exit.to_string())
                    .env("SNIPBOARD_TEST_INPUT_REPLY", reply);
                command
            };
            let text = "--literal 🌍\r\n\tsecond line\n$(not a command) `literal`\n";
            let result = paste_text(text, 0, fixture("wl-copy"), fixture("hyprctl"));
            assert_eq!(fs::read(directory.path().join("copied")).unwrap(), text.as_bytes());
            assert_eq!(fs::read_to_string(directory.path().join("copy-args")).unwrap(), "--type\ntext/plain;charset=utf-8\n");
            let input_args = directory.path().join("input-args");
            if copy_exit != 0 {
                assert!(!input_args.exists(), "Never paste the old clipboard after a copy failure");
                assert!(matches!(result, Err(Failure::Action(message)) if message.starts_with("Paste failed:")));
            } else {
                assert_eq!(fs::read_to_string(input_args).unwrap(), format!("eval\n{PASTE_LUA}\n"));
                if input_exit == 0 && reply == "ok" {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(result, Err(Failure::Action(message)) if message.starts_with("Text copied,")));
                }
            }
        }
    }
}

//! Persistent JSON-lines worker. Search never reads disk or launches processes.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::core::{MAX_KEYWORD, char_len, render, scratch_match, synthetic_library, synthetic_records};
use crate::json::{replace_lone_surrogates, to_ascii_string};
use crate::search::{Index, Kind, RESULT_LIMIT, Record};
use crate::storage::{Library, Snippet, Store, Usage, UsageEntry, read_library, read_usage, validate_library, write_usage};
use crate::{Error, Result, source_roots, sys};

pub const MAX_LINE: u64 = 512 * 1024;
pub const FATAL: &str = "Prototype storage unavailable or invalid; existing files preserved.";
const GENERIC: &str = "Action failed; check prototype storage or capability. Existing data was not reset.";
const INVALID_SNIPPET: &str = "Invalid snippet: use a nonblank name (256 max), body ≤65,536, notes ≤4,096, and an optional whitespace-free keyword ≤64. Keywords must not duplicate or prefix another; use at most one {cursor}.";

/// Only fixed, payload-free messages may cross the UI boundary.
#[derive(Debug)]
pub enum Failure {
    /// A library action failed; the response also carries the library state.
    Library(&'static str),
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
    child: Option<Child>,
}

impl Worker {
    pub fn new(root: &Path) -> Result<Self> {
        if !root.is_absolute() || root.components().any(|c| c == Component::ParentDir) {
            return Err(Error::Invalid("prototype data must be outside source at an absolute path"));
        }
        let resolved = resolve(root);
        if source_roots().iter().any(|source| resolved.starts_with(source)) {
            return Err(Error::Invalid("prototype data must be outside source at an absolute path"));
        }
        let mut store = Store::new(root)?;
        let lock = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(store.root.join("worker.lock"))?;
        lock.try_lock().map_err(|error| Error::Io(error.into()))?;
        let library = store.initialize(&synthetic_library(1000))?.clone();
        let usage = if store.usage_path.exists() { read_usage(&store.usage_path)? } else { Usage::new() };
        let index = Index::new(synthetic_records(&library, 10000), &usage);
        Ok(Worker { store, _lock: lock, usage, index, child: None })
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

    pub fn dispatch(&mut self, request: &Map<String, Value>) -> Response {
        let operation = request.get("op").and_then(Value::as_str).unwrap_or_default();
        match operation {
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
                Ok(object(json!({"body": item.body, "keyword": item.keyword})))
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
                let body = self.record(request)?.body.clone();
                let clipboard = text(request, "clipboard").ok_or(Failure::Generic)?;
                let result = render(&body, clipboard, None)?;
                self.copy(&result.text)?;
                Ok(object(json!({"message": "Copy requested (clipboard ownership unacknowledged).", "confirmed": false})))
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
                let body = self.record(request)?.body.clone();
                let clipboard = text(request, "clipboard").ok_or(Failure::Generic)?;
                let result = render(&body, clipboard, None)?;
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
        };
        response.insert("op".into(), request.get("op").cloned().unwrap_or(Value::Null));
        response.insert("seq".into(), request.get("seq").cloned().unwrap_or(json!(0)));
        let shutdown = request.get("op").and_then(Value::as_str) == Some("shutdown");
        (response, shutdown)
    }
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
    let worker = Arc::new(Mutex::new(Worker::new(root)?));
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
        let worker = Worker::new(directory.path()).unwrap();
        (directory, worker)
    }

    fn request(worker: &mut Worker, value: Value) -> Map<String, Value> {
        worker.handle(to_ascii_string(&value).as_bytes()).0
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
}

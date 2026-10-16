//! Worker protocol tests through the real binary, with disposable data directories.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use snipboard::storage::{read_library, read_usage, write_library};

const WORKER: &str = env!("CARGO_BIN_EXE_snipboard-worker");

fn data() -> tempfile::TempDir {
    let directory = tempfile::Builder::new().prefix("snipboard-").tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

struct Worker {
    process: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Worker {
    fn launch(directory: &Path) -> Self {
        let mut command = Command::new(WORKER);
        command.arg("--data").arg(directory);
        Self::spawn(&mut command)
    }

    fn spawn(command: &mut Command) -> Self {
        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = process.stdin.take();
        let output = BufReader::new(process.stdout.take().unwrap());
        let mut worker = Worker { process, input, output };
        assert_eq!(worker.read()["op"], "ready");
        worker
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        assert!(line.is_ascii(), "protocol output must be ASCII: {line}");
        serde_json::from_str(&line).unwrap()
    }

    fn send_line(&mut self, line: &str) -> Value {
        writeln!(self.input.as_mut().unwrap(), "{line}").unwrap();
        self.read()
    }

    fn request(&mut self, request: Value) -> Value {
        self.send_line(&request.to_string())
    }

    fn shutdown(mut self) -> String {
        assert_eq!(self.request(json!({"op": "shutdown"}))["ok"], true);
        assert!(self.process.wait().unwrap().success());
        let mut errors = String::new();
        self.process.stderr.take().unwrap().read_to_string(&mut errors).unwrap();
        errors
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(None) = self.process.try_wait() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn fields(snippet: &Value) -> Value {
    json!({"name": snippet["name"], "body": snippet["body"], "notes": snippet["notes"], "keyword": snippet["keyword"]})
}

#[test]
fn paste_protocol_renders_templates_and_sends_one_paste_gesture() {
    let directory = data();
    let commands = tempfile::tempdir().unwrap();
    for name in ["wl-copy", "hyprctl"] {
        let path = commands.path().join(name);
        fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name), &path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut paths = vec![commands.path().to_owned()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let mut command = Command::new(WORKER);
    command.arg("--data").arg(directory.path())
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("SNIPBOARD_TEST_OUTPUT", commands.path());
    let mut worker = Worker::spawn(&mut command);
    let state = worker.request(json!({"op": "snippet-get"}));
    let saved = worker.request(json!({"op": "snippet-save", "revision": state["revision"], "fields": {
        "name": "Paste fixture", "body": "🌍\r\n{clipboard}{cursor}\n", "notes": "", "keyword": ""
    }}));
    let response = worker.request(json!({"op": "paste", "seq": 42, "record": saved["record"], "clipboard": "literal {date}"}));
    assert_eq!(response, json!({"op": "paste", "seq": 42, "ok": true, "record": saved["record"]}));
    assert_eq!(fs::read_to_string(commands.path().join("copied")).unwrap(), "🌍\r\nliteral {date}\n");
    assert_eq!(fs::read_to_string(commands.path().join("input-args")).unwrap(), format!("eval\n{}\n", include_str!("../src/paste.lua")));
    assert!(worker.request(json!({"op": "paste", "record": "missing"})).get("error").is_some());
    worker.shutdown();
}

#[test]
fn protocol_scratch_search_and_persistence() {
    let directory = data();
    let mut worker = Worker::launch(directory.path());
    let result = worker.request(json!({"op": "search", "query": "email", "seq": 3}));
    assert_eq!(result["seq"], 3);
    assert_eq!(result["op"], "search");
    assert_eq!(result["rows"][0]["id"], "snippet-0000");
    assert!(result["rows"][0].get("body").is_none());
    assert!(result["matchMs"].is_f64());
    for (keyword, text) in [
        ("!email", "scratch@example.invalid"),
        ("!unicode", "Hello — Καλημέρα 🌍"),
        ("!multi", "First line\nSecond line\nThird line"),
    ] {
        let expanded = worker.request(json!({"op": "scratch", "prefix": keyword, "enabled": true, "context": "trusted-scratch"}));
        assert_eq!(expanded["expansion"]["text"], text);
        assert_eq!(expanded["expansion"]["remove"], keyword.len());
    }
    let variables = worker.request(json!({"op": "scratch", "prefix": " !vars", "enabled": true, "context": "trusted-scratch", "clipboard": "🌍"}));
    let text = variables["expansion"]["text"].as_str().unwrap();
    assert!(text.ends_with("\n🌍\nBefore after"), "{text}");
    let before_cursor = &text[..text.find("after").unwrap()];
    assert_eq!(variables["expansion"]["cursor"], before_cursor.encode_utf16().count());
    for (enabled, context) in [(false, "trusted-scratch"), (true, "password-manager")] {
        let refused = worker.request(json!({"op": "scratch", "prefix": "!email", "enabled": enabled, "context": context}));
        assert!(refused["expansion"].is_null());
    }
    assert!(worker.request(json!({"op": "scratch", "prefix": "x".repeat(66), "enabled": true, "context": "trusted-scratch"})).get("error").is_some());
    let rendered = worker.request(json!({"op": "render", "record": "snippet-0001"}));
    assert_eq!(rendered["text"], "Hello — Καλημέρα 🌍");
    assert_eq!(rendered["cursor"], "Hello — Καλημέρα 🌍".encode_utf16().count());
    worker.request(json!({"op": "used", "record": "snippet-0000"}));
    worker.request(json!({"op": "used", "record": "clipboard-00001"}));
    let usage = read_usage(&directory.path().join("usage.local.json")).unwrap();
    assert_eq!(usage["snippet-0000"].count, 1);
    assert_eq!(usage.len(), 1);
    assert_eq!(worker.shutdown(), "");
}

#[test]
fn invalid_requests_echo_sequence_without_stopping() {
    let directory = data();
    let mut worker = Worker::launch(directory.path());
    for request in [
        json!({"op": "nonsense", "seq": 1}),
        json!({"op": "search", "seq": 2, "query": 5}),
        json!({"op": "search", "seq": 3, "kind": "other"}),
        json!({"op": "preview", "seq": 4, "record": "missing"}),
        json!({"op": "render", "seq": 5}),
    ] {
        let response = worker.request(request.clone());
        assert_eq!(response["seq"], request["seq"]);
        assert!(response["error"].as_str().unwrap().starts_with("Action failed"));
    }
    let malformed = worker.send_line("not json");
    assert_eq!((malformed["op"].clone(), malformed["seq"].clone()), (Value::Null, json!(0)));
    assert_eq!(worker.request(json!({"op": "search", "seq": 6}))["seq"], 6);
}

#[test]
fn eof_sigterm_oversized_line_and_exclusive_worker() {
    let directory = data();
    let mut worker = Worker::launch(directory.path());
    let second = Command::new(WORKER).arg("--data").arg(directory.path()).stdin(Stdio::null()).output().unwrap();
    assert!(!second.status.success());
    let fatal: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(fatal["op"], "fatal");

    drop(worker.input.take());
    assert!(worker.process.wait().unwrap().success());

    let worker = Worker::launch(directory.path());
    // SAFETY: signalling the child we spawned and have not reaped.
    unsafe { libc::kill(worker.process.id() as libc::pid_t, libc::SIGTERM) };
    let mut worker = worker;
    assert!(worker.process.wait().unwrap().success());

    let mut worker = Worker::launch(directory.path());
    let input = worker.input.as_mut().unwrap();
    let _ = input.write_all(&vec![b'x'; 512 * 1024 + 2]);
    assert!(worker.process.wait().unwrap().success());
}

#[test]
fn source_and_relative_data_paths_are_fatal() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("forbidden-data");
    for path in [source.as_path(), Path::new("relative-data")] {
        let output = Command::new(WORKER).arg("--data").arg(path).stdin(Stdio::null()).output().unwrap();
        assert!(!output.status.success());
        assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap()["op"], "fatal");
    }
    assert!(!source.exists());
}

#[test]
fn duplicate_drafts_are_explicit_independent_and_revision_checked() {
    let directory = data();
    let path = directory.path().join("library.json");
    let mut worker = Worker::launch(directory.path());
    let original = worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}));
    worker.request(json!({"op": "used", "record": "snippet-0000"}));
    let before = fs::read(&path).unwrap();
    let draft = worker.request(json!({"op": "snippet-draft", "record": "snippet-0000", "seq": 17}));
    assert_eq!(draft["seq"], 17);
    assert_eq!(draft["snippet"]["id"], "");
    assert_eq!(draft["snippet"]["keyword"], "");
    assert_eq!(draft["snippet"]["body"], original["snippet"]["body"]);
    assert_eq!(draft["snippet"]["notes"], original["snippet"]["notes"]);
    assert_eq!(draft["snippet"]["name"], format!("{} (copy)", original["snippet"]["name"].as_str().unwrap()));
    assert_eq!(fs::read(&path).unwrap(), before, "Opening/canceling a draft never writes");
    let saved = worker.request(json!({"op": "snippet-save", "revision": draft["revision"], "fields": fields(&draft["snippet"])}));
    assert!(saved.get("error").is_none(), "{saved}");
    assert_ne!(saved["record"], "snippet-0000");
    assert_eq!(read_usage(&directory.path().join("usage.local.json")).unwrap().len(), 1);
    let stale = worker.request(json!({"op": "snippet-save", "revision": draft["revision"], "fields": fields(&draft["snippet"])}));
    assert!(stale["error"].as_str().unwrap().starts_with("Library changed"));
    assert_eq!(worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}))["snippet"], original["snippet"]);
    for source in [json!("missing"), json!(null), json!(5)] {
        assert!(worker.request(json!({"op": "snippet-draft", "record": source})).get("error").is_some());
    }
    worker.shutdown();
    let mut worker = Worker::launch(directory.path());
    assert_eq!(worker.request(json!({"op": "snippet-get", "record": saved["record"]}))["snippet"]["body"], draft["snippet"]["body"]);
}

#[test]
fn clipboard_to_snippet_draft_preserves_source_and_requires_save() {
    let directory = data();
    let path = directory.path().join("library.json");
    let mut worker = Worker::launch(directory.path());
    let source = worker.request(json!({"op": "preview", "record": "clipboard-00001"}));
    let before = fs::read(&path).unwrap();
    let draft = worker.request(json!({"op": "snippet-draft", "record": "clipboard-00001"}));
    assert_eq!(draft["snippet"]["id"], "");
    assert_eq!(draft["snippet"]["keyword"], "");
    assert_eq!(draft["snippet"]["notes"], "");
    assert_eq!(fs::read(&path).unwrap(), before);
    let saved = worker.request(json!({"op": "snippet-save", "revision": draft["revision"], "fields": fields(&draft["snippet"])}));
    assert!(saved.get("error").is_none(), "{saved}");
    assert_eq!(worker.request(json!({"op": "render", "record": saved["record"]}))["text"], source["body"]);
    assert_eq!(worker.request(json!({"op": "preview", "record": "clipboard-00001"})), source);
    fs::remove_file(&path).unwrap();
    worker.request(json!({"op": "library-reload"}));
    let readonly = worker.request(json!({"op": "snippet-draft", "record": "clipboard-00001"}));
    assert_eq!(readonly["readonly"], true);
    let refused = worker.request(json!({"op": "snippet-save", "revision": readonly["revision"], "fields": fields(&readonly["snippet"])}));
    assert!(refused.get("error").is_some());
    assert!(!path.exists());
}

#[test]
fn snippet_crud_exact_text_stable_usage_and_restart() {
    let directory = data();
    let usage_path = directory.path().join("usage.local.json");
    let mut worker = Worker::launch(directory.path());
    let new = worker.request(json!({"op": "snippet-get"}));
    assert_eq!(new["snippet"]["id"], "");
    let mut fields = json!({"name": "Fixture zephyreditor", "body": "\t🌍\r\n trailing \n{cursor}", "keyword": "!zephyr", "notes": "searchablenote"});
    let saved = worker.request(json!({"op": "snippet-save", "record": "", "fields": fields, "revision": new["revision"]}));
    let record = saved["record"].as_str().unwrap().to_owned();
    assert!(record.starts_with("snippet-") && record.len() == "snippet-".len() + 32);
    assert_ne!(saved["revision"], new["revision"]);
    assert_eq!(saved["deleted"], false);
    assert_eq!(worker.request(json!({"op": "search", "query": "zephyreditor"}))["rows"][0]["id"], record);
    assert_eq!(worker.request(json!({"op": "preview", "record": record}))["body"], fields["body"]);
    assert_eq!(worker.request(json!({"op": "search", "query": "searchablenote"}))["rows"][0]["id"], record);
    worker.request(json!({"op": "used", "record": record}));
    fields["name"] = json!("Renamed zephyreditor");
    let renamed = worker.request(json!({"op": "snippet-save", "record": record, "fields": fields, "revision": saved["revision"]}));
    assert_eq!(renamed["record"], record);
    let usage = read_usage(&usage_path).unwrap();
    assert_eq!(usage[&record].count, 1);
    assert_eq!(worker.shutdown(), "");

    let mut worker = Worker::launch(directory.path());
    let edited = worker.request(json!({"op": "snippet-get", "record": record}));
    let mut expected = fields.clone();
    expected["id"] = json!(record);
    assert_eq!(edited["snippet"], expected);
    let deleted = worker.request(json!({"op": "snippet-delete", "record": record, "revision": edited["revision"]}));
    assert_eq!(deleted["deleted"], true);
    assert!(worker.request(json!({"op": "snippet-get", "record": record})).get("error").is_some());
    assert_eq!(worker.request(json!({"op": "search", "query": "zephyreditor"}))["rows"], json!([]));
    let gone = worker.request(json!({"op": "scratch", "enabled": true, "context": "trusted-scratch", "prefix": "!zephyr"}));
    assert!(gone["expansion"].is_null());
    // Deleting a definition does not rewrite or export machine-local usage.
    assert_eq!(read_usage(&usage_path).unwrap(), usage);
    worker.shutdown();

    let mut worker = Worker::launch(directory.path());
    assert!(worker.request(json!({"op": "snippet-get", "record": record})).get("error").is_some());
}

#[test]
fn editor_validation_stale_drafts_and_clipboard_are_not_mutated() {
    let directory = data();
    let path = directory.path().join("library.json");
    let mut worker = Worker::launch(directory.path());
    let original = worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}));
    let mut fields = fields(&original["snippet"]);
    let before = fs::read(&path).unwrap();
    let mutations = [
        json!({"keyword": "!uni"}),
        json!({"body": "{cursor}{cursor}"}),
        json!({"name": "  "}),
        json!({"notes": "x".repeat(4097)}),
        json!({"body": "x".repeat(65537)}),
        json!({"keyword": "two words"}),
        json!({"keyword": "!unicode"}),
        json!({"name": 5}),
        json!({"id": "changed"}),
    ];
    let mut lines: Vec<String> = mutations
        .iter()
        .map(|mutation| {
            let mut changed = fields.clone();
            for (key, value) in mutation.as_object().unwrap() {
                changed[key] = value.clone();
            }
            json!({"op": "snippet-save", "seq": 1, "record": "snippet-0000", "revision": original["revision"], "fields": changed}).to_string()
        })
        .collect();
    // JavaScript can send lone surrogates; they must be rejected as invalid input, not as
    // an unparseable line that loses `seq`.
    for surrogate in [r"\ud800", r"\udc00x", r"a\ud800\u0041"] {
        let mut changed = fields.clone();
        changed["body"] = json!("SURROGATE");
        let line = json!({"op": "snippet-save", "seq": 1, "record": "snippet-0000", "revision": original["revision"], "fields": changed});
        lines.push(line.to_string().replace("SURROGATE", surrogate));
    }
    for line in &lines {
        let rejected = worker.send_line(line);
        assert_eq!(rejected["seq"], 1, "{line}");
        assert!(rejected.get("error").is_some(), "{line}");
        assert_eq!(rejected["readonly"], false, "{line}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    fields["name"] = json!("Renamed fixture");
    let saved = worker.request(json!({"op": "snippet-save", "record": "snippet-0000", "revision": original["revision"], "fields": fields}));
    assert!(saved.get("error").is_none(), "{saved}");
    for operation in ["snippet-save", "snippet-delete"] {
        let stale = worker.request(json!({"op": operation, "record": "snippet-0000", "revision": original["revision"], "fields": fields}));
        assert!(stale["error"].as_str().unwrap().starts_with("Library changed"));
    }
    for operation in ["snippet-save", "snippet-delete"] {
        let rejected = worker.request(json!({"op": operation, "record": "clipboard-00001", "revision": saved["revision"], "fields": fields}));
        assert!(rejected.get("error").is_some());
    }
    let missing = worker.request(json!({"op": "snippet-delete", "record": "", "revision": saved["revision"]}));
    assert!(missing.get("error").is_some());
    assert_eq!(worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}))["snippet"]["name"], fields["name"]);
}

#[test]
fn external_change_missing_and_invalid_reload_keep_last_good() {
    let directory = data();
    let path = directory.path().join("library.json");
    let mut worker = Worker::launch(directory.path());
    let original = worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}));
    let fields = fields(&original["snippet"]);
    let mut external = read_library(&path).unwrap();
    external.snippets[0].name = "External fixture".into();
    write_library(&path, &external).unwrap();
    let failed = worker.request(json!({"op": "snippet-save", "record": "snippet-0000", "revision": original["revision"], "fields": fields}));
    assert_eq!(failed["readonly"], true);
    assert_eq!(read_library(&path).unwrap(), external);
    assert_eq!(worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}))["snippet"], original["snippet"]);
    let refused = worker.request(json!({"op": "snippet-save", "record": "snippet-0000", "revision": failed["revision"], "fields": fields}));
    assert!(refused["error"].as_str().unwrap().starts_with("Library is read-only"));
    let loaded = worker.request(json!({"op": "library-reload"}));
    assert_eq!(loaded["readonly"], false);
    assert_ne!(loaded["revision"], original["revision"]);
    assert_eq!(worker.request(json!({"op": "search", "query": "External fixture"}))["rows"][0]["id"], "snippet-0000");
    fs::write(&path, "invalid").unwrap();
    assert_eq!(worker.request(json!({"op": "library-reload"}))["readonly"], true);
    assert_eq!(worker.request(json!({"op": "snippet-get", "record": "snippet-0000"}))["snippet"]["name"], "External fixture");
    fs::remove_file(&path).unwrap();
    assert!(worker.request(json!({"op": "library-reload"})).get("error").is_some());
    assert!(!path.exists());
    write_library(&path, &external).unwrap();
    assert_eq!(worker.request(json!({"op": "library-reload"}))["readonly"], false);
}

#[test]
fn reads_libraries_written_by_the_python_prototype() {
    // Python's json.dumps(indent=2, ensure_ascii=False) with its own key order and an
    // integer timestamp must remain loadable after the rewrite.
    let directory = data();
    let library = "{\n  \"schemaVersion\": 1,\n  \"snippets\": [\n    {\n      \"id\": \"snippet-0000\",\n      \"name\": \"Email\",\n      \"keyword\": \"!email\",\n      \"body\": \"Καλημέρα 🌍\",\n      \"notes\": \"\"\n    },\n    {\n      \"name\": \"Edited\",\n      \"body\": \"x\",\n      \"notes\": \"\",\n      \"keyword\": \"\",\n      \"id\": \"snippet-0a1b\"\n    }\n  ],\n  \"portable\": {\n    \"dateFormat\": \"YYYY-MM-DD\"\n  }\n}\n";
    let usage = "{\n  \"snippet-0000\": {\n    \"count\": 3,\n    \"last\": 1760000000\n  },\n  \"snippet-0a1b\": {\n    \"count\": 1,\n    \"last\": 1760000000.25\n  }\n}\n";
    for (name, content) in [("library.json", library), ("usage.local.json", usage), ("initialized", "")] {
        let path = directory.path().join(name);
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mut worker = Worker::launch(directory.path());
    assert_eq!(worker.request(json!({"op": "search", "query": "edited"}))["rows"][0]["id"], "snippet-0a1b");
    assert_eq!(worker.request(json!({"op": "preview", "record": "snippet-0000"}))["body"], "Καλημέρα 🌍");
    worker.request(json!({"op": "used", "record": "snippet-0000"}));
    assert_eq!(read_usage(&directory.path().join("usage.local.json")).unwrap()["snippet-0000"].count, 4);
}

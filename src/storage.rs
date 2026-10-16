//! Versioned portable definitions and separate machine-local usage, never history.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::core::{MAX_BODY, char_len, cursor_markers, strip, validate_keywords};
use crate::json::to_ascii_string;
use crate::{Error, Result, sys};

pub const MAX_FILE: u64 = 8 * 1024 * 1024;
const DATE_FORMAT: &str = "YYYY-MM-DD";

// Derived struct deserializers reject unknown, missing, and duplicate keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub keyword: String,
    pub body: String,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Portable {
    pub date_format: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Library {
    pub schema_version: u64,
    pub snippets: Vec<Snippet>,
    pub portable: Portable,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            schema_version: 1,
            snippets: Vec::new(),
            portable: Portable { date_format: DATE_FORMAT.to_owned() },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageEntry {
    pub count: u64,
    pub last: f64,
}

pub type Usage = BTreeMap<String, UsageEntry>;

/// Usage keys are record IDs, so the map needs its own duplicate-key check.
struct StrictUsage(Usage);

impl<'de> Deserialize<'de> for StrictUsage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct UsageVisitor;
        impl<'de> Visitor<'de> for UsageVisitor {
            type Value = StrictUsage;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a usage object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<StrictUsage, A::Error> {
                let mut usage = Usage::new();
                while let Some((key, entry)) = map.next_entry::<String, UsageEntry>()? {
                    if usage.insert(key, entry).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(StrictUsage(usage))
            }
        }
        deserializer.deserialize_map(UsageVisitor)
    }
}

/// Stable IDs: ASCII alphanumeric start, then up to 127 of `[A-Za-z0-9._-]`.
pub fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphanumeric())
        && id.len() <= 128
        && bytes.all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub fn validate_library(library: &Library) -> Result<()> {
    if library.schema_version != 1 {
        return Err(Error::Invalid("unsupported schema"));
    }
    if library.snippets.len() > 10000 {
        return Err(Error::Invalid("invalid snippet collection"));
    }
    let mut ids = std::collections::HashSet::new();
    for snippet in &library.snippets {
        let limits = [
            (&snippet.id, 128),
            (&snippet.name, 256),
            (&snippet.body, MAX_BODY),
            (&snippet.notes, 4096),
            (&snippet.keyword, 64),
        ];
        if limits.iter().any(|(value, maximum)| char_len(value) > *maximum || value.contains('\0')) {
            return Err(Error::Invalid("invalid snippet value"));
        }
        if !valid_id(&snippet.id) || !ids.insert(snippet.id.as_str()) || strip(&snippet.name).is_empty() {
            return Err(Error::Invalid("invalid or duplicate stable id"));
        }
        if cursor_markers(&snippet.body) > 1 {
            return Err(Error::Invalid("multiple cursor markers"));
        }
    }
    if library.portable.date_format != DATE_FORMAT {
        return Err(Error::Invalid("unsupported portable settings"));
    }
    validate_keywords(library.snippets.iter().map(|s| s.keyword.as_str()))
}

pub fn validate_usage(usage: &Usage) -> Result<()> {
    if usage.len() > 10000 {
        return Err(Error::Invalid("invalid usage"));
    }
    for (id, entry) in usage {
        if !valid_id(id) {
            return Err(Error::Invalid("invalid usage entry"));
        }
        if entry.count > 1_000_000_000_000 {
            return Err(Error::Invalid("invalid usage count"));
        }
        if !entry.last.is_finite() || entry.last < 0.0 {
            return Err(Error::Invalid("invalid usage timestamp"));
        }
    }
    Ok(())
}

pub fn parse_library(raw: &[u8]) -> Result<Library> {
    let library = serde_json::from_slice(raw).map_err(|_| Error::Invalid("invalid library JSON"))?;
    validate_library(&library)?;
    Ok(library)
}

pub fn parse_usage(raw: &[u8]) -> Result<Usage> {
    let StrictUsage(usage) = serde_json::from_slice(raw).map_err(|_| Error::Invalid("invalid usage JSON"))?;
    validate_usage(&usage)?;
    Ok(usage)
}

/// Read a bounded, owner-only regular file without following symlinks.
fn read_private(path: &Path) -> Result<Vec<u8>> {
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path)?;
    let info = file.metadata()?;
    if !info.is_file() || info.uid() != sys::uid() || info.mode() & 0o077 != 0 {
        return Err(Error::Invalid("data must be an owner-only regular file"));
    }
    let mut raw = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_FILE {
        return Err(Error::Invalid("file exceeds limit"));
    }
    Ok(raw)
}

pub fn read_library(path: &Path) -> Result<Library> {
    parse_library(&read_private(path)?)
}

pub fn read_usage(path: &Path) -> Result<Usage> {
    parse_usage(&read_private(path)?)
}

#[cfg(test)]
thread_local! {
    /// Test-only fault injection for the atomic replacement step.
    pub static FAIL_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_REPLACE.get() {
        return Err(io::Error::other("synthetic failure"));
    }
    fs::rename(from, to)
}

/// Removes the temporary file on every path that did not rename it into place.
struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Same-directory temporary file, fsync, atomic rename, then directory fsync.
fn atomic_write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut encoded = serde_json::to_vec_pretty(value).map_err(|_| Error::Invalid("unserializable value"))?;
    encoded.push(b'\n');
    if encoded.len() as u64 > MAX_FILE {
        return Err(Error::Invalid("file exceeds limit"));
    }
    if path.symlink_metadata().is_ok_and(|info| info.file_type().is_symlink()) {
        return Err(Error::Invalid("refusing symlink"));
    }
    let directory = path.parent().ok_or(Error::Invalid("path has no directory"))?;
    let temporary = Temporary(directory.join(format!(".snipboard-{}", sys::random_hex(8)?)));
    let mut stream = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary.0)?;
    stream.write_all(&encoded)?;
    stream.sync_all()?;
    drop(stream);
    replace(&temporary.0, path)?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

pub fn write_library(path: &Path, library: &Library) -> Result<()> {
    validate_library(library)?;
    atomic_write(path, library)
}

pub fn write_usage(path: &Path, usage: &Usage) -> Result<()> {
    validate_usage(usage)?;
    atomic_write(path, usage)
}

/// Content hash of the library: canonical (sorted-key, ASCII, compact) JSON, SHA-256.
pub fn revision(library: &Library) -> String {
    let canonical = serde_json::to_value(library).expect("library serializes");
    Sha256::digest(to_ascii_string(&canonical))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct Store {
    pub root: PathBuf,
    pub library_path: PathBuf,
    pub usage_path: PathBuf,
    pub library: Library,
    pub readonly: bool,
}

impl Store {
    pub fn new(root: &Path) -> Result<Self> {
        DirBuilder::new().recursive(true).mode(0o700).create(root)?;
        let info = root.symlink_metadata()?;
        if !info.is_dir() || info.uid() != sys::uid() || info.mode() & 0o077 != 0 {
            return Err(Error::Invalid("prototype root must be an owner-only real directory"));
        }
        Ok(Store {
            root: root.to_path_buf(),
            library_path: root.join("library.json"),
            usage_path: root.join("usage.local.json"),
            library: Library::default(),
            readonly: false,
        })
    }

    /// Seed the fixture only on first run; a later missing library is an error, not a reset.
    pub fn initialize(&mut self, fixture: &Library) -> Result<&Library> {
        let marker = self.root.join("initialized");
        if !marker.exists() && !self.library_path.exists() {
            write_library(&self.library_path, fixture)?;
            OpenOptions::new().write(true).create_new(true).mode(0o600).open(&marker)?;
        }
        self.reload()
    }

    pub fn reload(&mut self) -> Result<&Library> {
        match read_library(&self.library_path) {
            Ok(fresh) => {
                self.library = fresh;
                self.readonly = false;
                Ok(&self.library)
            }
            Err(error) => {
                self.readonly = true;
                Err(error)
            }
        }
    }

    pub fn revision(&self) -> String {
        revision(&self.library)
    }

    pub fn save(&mut self, library: Library) -> Result<()> {
        if self.readonly {
            return Err(Error::Invalid("storage is read-only after a failed reload"));
        }
        let current = read_library(&self.library_path).inspect_err(|_| self.readonly = true)?;
        if current != self.library {
            self.readonly = true;
            return Err(Error::Invalid("external change requires explicit reload/review"));
        }
        if let Err(error) = write_library(&self.library_path, &library) {
            // Replacement may already have happened before a directory fsync failure.
            // Require explicit reload rather than letting a retry overwrite blindly.
            if matches!(error, Error::Io(_)) {
                self.readonly = true;
            }
            return Err(error);
        }
        self.library = library;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::synthetic_library;
    use serde_json::{Value, json};
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        _directory: tempfile::TempDir,
        root: PathBuf,
        store: Store,
        library: Library,
    }

    fn fixture() -> Fixture {
        let directory = tempfile::Builder::new().prefix("snipboard-").tempdir().unwrap();
        let root = directory.path().to_path_buf();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mut store = Store::new(&root).unwrap();
        let library = store.initialize(&synthetic_library(4)).unwrap().clone();
        Fixture { _directory: directory, root, store, library }
    }

    fn temporaries(root: &Path) -> usize {
        fs::read_dir(root)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(".snipboard-"))
            .count()
    }

    #[test]
    fn roundtrip_exact_body_stable_id_and_usage() {
        let mut f = fixture();
        let mut fresh = f.library.clone();
        fresh.snippets[0].body = "\t🌍\n trailing \n".into();
        fresh.snippets[0].name = "Renamed".into();
        f.store.save(fresh.clone()).unwrap();
        assert_eq!(*Store::new(&f.root).unwrap().reload().unwrap(), fresh);
        let usage: Usage = [("snippet-0000".to_owned(), UsageEntry { count: 9, last: 1000.0 })].into();
        write_usage(&f.store.usage_path, &usage).unwrap();
        assert_eq!(read_usage(&f.store.usage_path).unwrap(), usage);
        let stored: Value = serde_json::from_slice(&fs::read(&f.store.library_path).unwrap()).unwrap();
        assert!(stored.get("usage").is_none());
        assert_eq!(fs::metadata(&f.store.library_path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn invalid_missing_preserve_last_good_and_refuse_writes() {
        let mut f = fixture();
        fs::write(&f.store.library_path, "broken").unwrap();
        assert!(matches!(f.store.reload(), Err(Error::Invalid(_))));
        assert_eq!(f.store.library, f.library);
        assert!(f.store.readonly);
        assert!(f.store.save(f.library.clone()).is_err());
        assert_eq!(fs::read_to_string(&f.store.library_path).unwrap(), "broken");
        fs::remove_file(&f.store.library_path).unwrap();
        let mut again = Store::new(&f.root).unwrap();
        assert!(matches!(again.initialize(&synthetic_library(1000)), Err(Error::Io(_))));
        assert!(!f.store.library_path.exists());
    }

    #[test]
    fn atomic_failure_retains_original_and_cleans_temp() {
        let mut f = fixture();
        let original = fs::read(&f.store.library_path).unwrap();
        FAIL_REPLACE.set(true);
        let result = f.store.save(f.library.clone());
        FAIL_REPLACE.set(false);
        assert!(matches!(result, Err(Error::Io(_))));
        assert!(f.store.readonly);
        assert_eq!(fs::read(&f.store.library_path).unwrap(), original);
        assert_eq!(temporaries(&f.root), 0);
    }

    #[test]
    fn schema_duplicate_ids_keywords_private_fields_and_versions() {
        let f = fixture();
        let base = serde_json::to_value(&f.library).unwrap();
        let mutations: [fn(&mut Value); 7] = [
            |x| x["schemaVersion"] = json!(true),
            |x| x["schemaVersion"] = json!(1.0),
            |x| x["history"] = json!([]),
            |x| {
                let first = x["snippets"][0].clone();
                x["snippets"].as_array_mut().unwrap().push(first)
            },
            |x| x["snippets"][1]["keyword"] = json!("!email-long"),
            |x| x["portable"]["machinePath"] = json!("/private"),
            |x| x["snippets"][0]["id"] = json!("-leading"),
        ];
        for mutate in mutations {
            let mut invalid = base.clone();
            mutate(&mut invalid);
            assert!(parse_library(&serde_json::to_vec(&invalid).unwrap()).is_err(), "{invalid}");
        }
        assert!(parse_library(&serde_json::to_vec(&base).unwrap()).is_ok());
    }

    #[test]
    fn symlink_permissions_duplicate_json_keys() {
        let f = fixture();
        let link = f.root.join("link");
        std::os::unix::fs::symlink(&f.store.library_path, &link).unwrap();
        assert!(matches!(read_library(&link), Err(Error::Io(_))));
        assert!(matches!(write_library(&link, &f.library), Err(Error::Invalid(_))));
        fs::set_permissions(&f.store.library_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(read_library(&f.store.library_path), Err(Error::Invalid(_))));
        fs::set_permissions(&f.store.library_path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut duplicated = String::from_utf8(fs::read(&f.store.library_path).unwrap()).unwrap();
        duplicated = duplicated.replacen("\"schemaVersion\": 1,", "\"schemaVersion\": 1, \"schemaVersion\": 1,", 1);
        fs::write(&f.store.library_path, duplicated).unwrap();
        assert!(read_library(&f.store.library_path).is_err());
        let duplicate_usage = br#"{"a":{"count":1,"last":0},"a":{"count":2,"last":0}}"#;
        assert!(parse_usage(duplicate_usage).is_err());
    }

    #[test]
    fn usage_validation() {
        assert!(parse_usage(br#"{"a":{"count":1,"last":1.5},"b":{"count":0,"last":0}}"#).is_ok());
        for invalid in [
            r#"{"a":{"count":1.0,"last":0}}"#,
            r#"{"a":{"count":-1,"last":0}}"#,
            r#"{"a":{"count":1000000000001,"last":0}}"#,
            r#"{"a":{"count":1,"last":-1}}"#,
            r#"{"a":{"count":1,"last":true}}"#,
            r#"{"a":{"count":1}}"#,
            r#"{"-a":{"count":1,"last":0}}"#,
            r#"[]"#,
        ] {
            assert!(parse_usage(invalid.as_bytes()).is_err(), "{invalid}");
        }
    }

    #[test]
    fn external_change_is_not_silently_overwritten() {
        let mut f = fixture();
        let mut external = f.library.clone();
        external.snippets[0].body = "External scratch change".into();
        write_library(&f.store.library_path, &external).unwrap();
        assert!(f.store.save(f.library.clone()).is_err());
        assert!(f.store.readonly);
        assert_eq!(read_library(&f.store.library_path).unwrap(), external);
    }

    #[test]
    fn missing_storage_on_save_becomes_readonly() {
        let mut f = fixture();
        fs::remove_file(&f.store.library_path).unwrap();
        assert!(matches!(f.store.save(f.library.clone()), Err(Error::Io(_))));
        assert!(f.store.readonly);
        assert_eq!(f.store.library, f.library);
        assert!(!f.store.library_path.exists());
    }

    #[test]
    fn template_validation_and_revision() {
        let mut f = fixture();
        let revision = f.store.revision();
        let mut fresh = f.library.clone();
        fresh.snippets[0].body = "{{cursor}} {cursor}".into();
        f.store.save(fresh.clone()).unwrap();
        assert_ne!(revision, f.store.revision());
        assert_eq!(*Store::new(&f.root).unwrap().reload().unwrap(), fresh);
        let invalid: [fn(&mut Snippet); 4] = [
            |s| s.body = "{cursor}{cursor}".into(),
            |s| s.name = " \t\n".into(),
            |s| s.body = "nul\0".into(),
            |s| s.notes = "x".repeat(4097),
        ];
        for mutate in invalid {
            let mut library = fresh.clone();
            mutate(&mut library.snippets[0]);
            assert!(matches!(f.store.save(library), Err(Error::Invalid(_))));
            assert!(!f.store.readonly);
        }
        assert_eq!(f.store.library, fresh);
    }

    #[test]
    fn revision_is_canonical_json_sha256() {
        // Matches Python's json.dumps(sort_keys=True, ensure_ascii=True, separators=(",", ":")).
        let library = Library {
            snippets: vec![Snippet { id: "a".into(), name: "é".into(), keyword: String::new(), body: String::new(), notes: String::new() }],
            ..Library::default()
        };
        let canonical = serde_json::to_value(&library).unwrap();
        assert_eq!(
            to_ascii_string(&canonical),
            r#"{"portable":{"dateFormat":"YYYY-MM-DD"},"schemaVersion":1,"snippets":[{"body":"","id":"a","keyword":"","name":"\u00e9","notes":""}]}"#
        );
        assert_eq!(revision(&library).len(), 64);
    }
}

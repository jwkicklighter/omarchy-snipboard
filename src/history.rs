//! Private text history, separate from the portable snippet library.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::{MAX_BODY, char_len};
use crate::search::Record;
use crate::storage::{MAX_FILE, atomic_write, read_private};
use crate::{Error, Result};

pub const MAX_ITEMS: usize = 1000;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_AGE: f64 = 30.0 * 86400.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    body: String,
    created: f64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    entries: Vec<Entry>,
}

fn valid_text(text: &str) -> bool {
    !text.is_empty() && !text.contains('\0') && char_len(text) <= MAX_BODY
}

fn id(text: &str) -> String {
    format!("clipboard-{:x}", Sha256::digest(text.as_bytes()))
}

impl History {
    pub fn read(path: &Path, now: f64) -> Result<Self> {
        let mut history: Self = match read_private(path) {
            Ok(raw) => serde_json::from_slice(&raw).map_err(|_| Error::Invalid("invalid history JSON"))?,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let mut ids = HashSet::new();
        if history.entries.len() > MAX_ITEMS
            || history.entries.iter().map(|e| e.body.len()).sum::<usize>() > MAX_BYTES
            || history.entries.iter().any(|e| !valid_text(&e.body) || !e.created.is_finite() || e.created < 0.0 || !ids.insert(id(&e.body)))
        {
            return Err(Error::Invalid("invalid history"));
        }
        let previous = history.entries.len();
        history.prune(now);
        if history.entries.len() != previous {
            atomic_write(path, &history)?;
        }
        Ok(history)
    }

    fn prune(&mut self, now: f64) {
        self.entries.retain(|entry| now - entry.created <= MAX_AGE);
        self.entries.sort_by(|a, b| b.created.total_cmp(&a.created));
        self.entries.truncate(MAX_ITEMS);
        let mut bytes = 0;
        let mut encoded = 128;
        self.entries.retain(|entry| {
            bytes += entry.body.len();
            // Account for JSON escapes too, with room for timestamp/formatting.
            encoded += serde_json::to_vec(&entry.body).expect("strings serialize").len() + 128;
            bytes <= MAX_BYTES && encoded <= MAX_FILE as usize
        });
    }

    /// Persist before publishing. A failed write leaves the live history unchanged.
    pub fn capture(&mut self, path: &Path, text: &str, now: f64) -> Result<String> {
        if !valid_text(text) {
            return Err(Error::Invalid("invalid clipboard text"));
        }
        let mut fresh = self.clone();
        fresh.entries.retain(|entry| entry.body != text);
        fresh.entries.push(Entry { body: text.to_owned(), created: now });
        fresh.prune(now);
        atomic_write(path, &fresh)?;
        *self = fresh;
        Ok(id(text))
    }

    /// Delete only history text, persisting before changing the live history.
    pub fn delete(&mut self, path: &Path, record: &str) -> Result<()> {
        let mut fresh = self.clone();
        let previous = fresh.entries.len();
        fresh.entries.retain(|entry| id(&entry.body) != record);
        if fresh.entries.len() == previous {
            return Err(Error::Invalid("clipboard entry no longer exists"));
        }
        atomic_write(path, &fresh)?;
        *self = fresh;
        Ok(())
    }

    pub fn records(&self) -> Vec<Record> {
        self.entries.iter().map(|entry| Record {
            id: id(&entry.body),
            name: entry.body.lines().next().unwrap_or_default().chars().take(100).collect(),
            body: entry.body.clone(),
            created: entry.created,
            ..Record::default()
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FAIL_REPLACE;

    #[test]
    fn exact_deduplication_retention_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.local.json");
        let mut history = History::default();
        let first = history.capture(&path, "🌍\r\n{date}\n ", 1.0).unwrap();
        history.capture(&path, "second", 2.0).unwrap();
        assert_eq!(history.capture(&path, "🌍\r\n{date}\n ", 3.0).unwrap(), first);
        assert_eq!(history.records().len(), 2);
        let restored = History::read(&path, 4.0).unwrap();
        assert_eq!(restored.records(), history.records());
        assert_eq!(restored.records()[0].body, "🌍\r\n{date}\n ");
        assert!(History::read(&path, MAX_AGE + 4.0).unwrap().records().is_empty());
        for i in 0..MAX_ITEMS + 5 {
            history.entries.push(Entry { body: format!("item {i}"), created: 10.0 + i as f64 });
        }
        history.prune(2000.0);
        assert_eq!(history.records().len(), MAX_ITEMS);
        history.entries = (0..100).map(|i| Entry { body: format!("{i}{}", "x".repeat(MAX_BODY - 3)), created: i as f64 }).collect();
        history.prune(2000.0);
        assert!(history.entries.iter().map(|e| e.body.len()).sum::<usize>() <= MAX_BYTES);
        history.entries = (0..100).map(|i| Entry { body: format!("{i}{}", "\u{1}".repeat(MAX_BODY - 3)), created: i as f64 }).collect();
        history.prune(2000.0);
        assert!(serde_json::to_vec_pretty(&history).unwrap().len() as u64 <= MAX_FILE);
    }

    #[test]
    fn deletion_is_atomic_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.local.json");
        let mut history = History::default();
        let first = history.capture(&path, "🌍\r\n{date}\n ", 1.0).unwrap();
        let second = history.capture(&path, "keep", 2.0).unwrap();
        let before = std::fs::read(&path).unwrap();
        let records = history.records();
        for invalid in ["", "missing", "snippet-0000"] {
            assert!(history.delete(&path, invalid).is_err());
        }
        FAIL_REPLACE.set(true);
        let failed = history.delete(&path, &first);
        FAIL_REPLACE.set(false);
        assert!(failed.is_err());
        assert_eq!(history.records(), records);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        history.delete(&path, &first).unwrap();
        assert_eq!(history.records().len(), 1);
        assert_eq!(history.records()[0].id, second);
        assert_eq!(History::read(&path, 3.0).unwrap().records(), history.records());
        assert!(history.delete(&path, &first).is_err());
        history.delete(&path, &second).unwrap();
        assert!(History::read(&path, 3.0).unwrap().records().is_empty());
        // A later clipboard event can record the same text again.
        assert_eq!(history.capture(&path, "keep", 4.0).unwrap(), second);
    }

    #[test]
    fn invalid_and_failed_writes_preserve_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.local.json");
        let mut history = History::default();
        history.capture(&path, "original", 1.0).unwrap();
        let before = std::fs::read(&path).unwrap();
        for invalid in ["".to_owned(), "nul\0".to_owned(), "x".repeat(MAX_BODY + 1)] {
            assert!(history.capture(&path, &invalid, 2.0).is_err());
        }
        FAIL_REPLACE.set(true);
        let failed = history.capture(&path, "unsaved", 3.0);
        FAIL_REPLACE.set(false);
        assert!(failed.is_err());
        assert_eq!(history.records()[0].body, "original");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(History::read(&path, 4.0).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid");
    }
}

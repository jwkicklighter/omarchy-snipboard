//! In-memory fuzzy search. Searching never reads disk or launches processes.
//!
//! Ranking: exact, prefix, substring, then subsequence tiers, with keyword/name matches
//! ahead of incidental notes/body matches. Relevance always precedes learned usage
//! (frequency with 14-day exponential recency decay); clipboard records keep chronology;
//! stable IDs break remaining ties.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::core::{char_len, strip};
use crate::storage::{Snippet, Usage};
use crate::{Error, Result};

/// Each field indexes only its first case-folded characters; longer text is not searchable.
pub const INDEX_LIMIT: usize = 512;
pub const MAX_QUERY: usize = 256;
pub const RESULT_LIMIT: usize = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Clipboard,
    Snippet,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Clipboard => "clipboard",
            Kind::Snippet => "snippet",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub keyword: String,
    pub body: String,
    pub notes: String,
    /// Clipboard chronology; newer records sort first on equal relevance.
    pub created: f64,
}

impl From<&Snippet> for Record {
    fn from(snippet: &Snippet) -> Self {
        Record {
            id: snippet.id.clone(),
            kind: Kind::Snippet,
            name: snippet.name.clone(),
            keyword: snippet.keyword.clone(),
            body: snippet.body.clone(),
            notes: snippet.notes.clone(),
            created: 0.0,
        }
    }
}

/// Characters present in a record or query: an ASCII bitmap plus sorted other characters.
#[derive(Default)]
struct Alphabet {
    ascii: u128,
    other: Vec<char>,
}

impl Alphabet {
    fn add(&mut self, character: char) {
        if character.is_ascii() {
            self.ascii |= 1 << character as u32;
        } else {
            self.other.push(character);
        }
    }

    fn finish(mut self) -> Self {
        self.other.sort_unstable();
        self.other.dedup();
        self
    }

    fn contains_all(&self, needed: &Alphabet) -> bool {
        needed.ascii & !self.ascii == 0 && needed.other.iter().all(|c| self.other.binary_search(c).is_ok())
    }
}

struct Entry {
    /// keyword, name, notes, body — case-folded and truncated.
    fields: [String; 4],
    lengths: [usize; 4],
    alphabet: Alphabet,
    count: f64,
    last: f64,
}

pub fn fold(text: &str) -> String {
    caseless::default_case_fold_str(text)
}

/// Lower is better: (tier, spread).
fn field_match(query: &str, query_length: usize, value: &str, value_length: usize) -> Option<(u8, usize)> {
    if query == value {
        return Some((0, 0));
    }
    if value.starts_with(query) {
        return Some((1, value_length - query_length));
    }
    if let Some(offset) = value.find(query) {
        return Some((2, char_len(&value[..offset])));
    }
    let mut available = value.chars().enumerate();
    let (mut first, mut last) = (None, 0);
    for wanted in query.chars() {
        let (position, _) = available.find(|&(_, character)| character == wanted)?;
        first.get_or_insert(position);
        last = position;
    }
    Some((3, last - first? + 1 - query_length))
}

struct Score<'a> {
    textual: (u8, u8, u8, usize),
    learned: f64,
    chronology: f64,
    id: &'a str,
}

impl Score<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.textual
            .cmp(&other.textual)
            // Higher usage first. partial_cmp keeps 0.0 and -0.0 equal; values are finite.
            .then_with(|| other.learned.partial_cmp(&self.learned).unwrap_or(Ordering::Equal))
            .then_with(|| other.chronology.partial_cmp(&self.chronology).unwrap_or(Ordering::Equal))
            .then_with(|| self.id.cmp(other.id))
    }
}

pub struct Index {
    records: Vec<Record>,
    by_id: HashMap<String, usize>,
    entries: Vec<Entry>,
}

impl Index {
    pub fn new(records: Vec<Record>, usage: &Usage) -> Self {
        let entries = records
            .iter()
            .map(|record| {
                let fields = [&record.keyword, &record.name, &record.notes, &record.body]
                    .map(|value| fold(value).chars().take(INDEX_LIMIT).collect::<String>());
                let mut alphabet = Alphabet::default();
                fields.iter().flat_map(|field| field.chars()).for_each(|c| alphabet.add(c));
                Entry {
                    lengths: fields.each_ref().map(|field| char_len(field)),
                    fields,
                    alphabet: alphabet.finish(),
                    count: 0.0,
                    last: 0.0,
                }
            })
            .collect();
        let by_id = records.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect();
        let mut index = Index { records, by_id, entries };
        index.update_usage(usage);
        index
    }

    pub fn update_usage(&mut self, usage: &Usage) {
        for (record, entry) in self.records.iter().zip(&mut self.entries) {
            let stat = usage.get(&record.id);
            entry.count = stat.map_or(0.0, |s| s.count as f64);
            entry.last = stat.map_or(0.0, |s| s.last);
        }
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn get(&self, id: &str) -> Option<&Record> {
        self.by_id.get(id).map(|&i| &self.records[i])
    }

    pub fn search(&self, query: &str, kind: &str, limit: usize, now: f64) -> Result<Vec<&Record>> {
        let filter = match kind {
            "all" => None,
            "clipboard" => Some(Kind::Clipboard),
            "snippet" => Some(Kind::Snippet),
            _ => return Err(Error::Invalid("invalid search")),
        };
        if char_len(query) > MAX_QUERY {
            return Err(Error::Invalid("invalid search"));
        }
        let query = fold(strip(query));
        let query_length = char_len(&query);
        let mut needed = Alphabet::default();
        query.chars().for_each(|c| needed.add(c));
        let needed = needed.finish();

        let mut candidates = Vec::new();
        for (record, entry) in self.records.iter().zip(&self.entries) {
            if filter.is_some_and(|kind| kind != record.kind) {
                continue;
            }
            let textual = if query.is_empty() {
                (u8::from(record.kind == Kind::Snippet), 0, 0, 0)
            } else {
                if !entry.alphabet.contains_all(&needed) {
                    continue;
                }
                let best = (0..4u8)
                    .filter_map(|field| {
                        let i = usize::from(field);
                        field_match(&query, query_length, &entry.fields[i], entry.lengths[i])
                            .map(|(tier, spread)| (u8::from(field >= 2), tier, field, spread))
                    })
                    .min();
                match best {
                    Some(best) => best,
                    None => continue,
                }
            };
            let learned = match record.kind {
                Kind::Snippet => {
                    let age = (now - entry.last).max(0.0) / 86400.0;
                    entry.count.ln_1p() * (-age / 14.0).exp()
                }
                Kind::Clipboard => 0.0,
            };
            let chronology = if record.kind == Kind::Clipboard { record.created } else { 0.0 };
            candidates.push((Score { textual, learned, chronology, id: &record.id }, record));
        }

        fn smallest<'r>(mut group: Vec<(Score<'r>, &'r Record)>, count: usize) -> Vec<&'r Record> {
            if count < group.len() {
                group.select_nth_unstable_by(count, |a, b| a.0.cmp(&b.0));
                group.truncate(count);
            }
            group.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            group.into_iter().map(|(_, record)| record).collect()
        }
        if query.is_empty() && filter.is_none() {
            // Empty All search reserves half the places for each group.
            let (clips, snippets) = candidates.into_iter().partition(|(_, r)| r.kind == Kind::Clipboard);
            let mut rows = smallest(clips, limit / 2);
            rows.extend(smallest(snippets, limit / 2));
            return Ok(rows);
        }
        Ok(smallest(candidates, limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{synthetic_library, synthetic_records};
    use crate::storage::UsageEntry;

    fn snippet(id: &str, name: &str, body: &str) -> Record {
        Record { id: id.into(), kind: Kind::Snippet, name: name.into(), body: body.into(), ..Record::default() }
    }

    fn usage(entries: &[(&str, u64, f64)]) -> Usage {
        entries.iter().map(|&(id, count, last)| (id.to_owned(), UsageEntry { count, last })).collect()
    }

    fn ids(rows: Vec<&Record>) -> Vec<&str> {
        rows.into_iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn exact_relevance_before_usage() {
        let index = Index::new(
            vec![snippet("a", "Email", "text"), snippet("b", "Email address", "text"), snippet("c", "Else", "Email")],
            &usage(&[("b", 999_999, 1000.0), ("c", 999_999, 1000.0)]),
        );
        assert_eq!(ids(index.search("email", "all", 60, 1000.0).unwrap()), ["a", "b", "c"]);
    }

    #[test]
    fn usage_frequency_decay_and_stability() {
        let records = vec![snippet("a", "Email", "text"), snippet("b", "Email", "text")];
        let mut index = Index::new(records, &usage(&[("b", 10, 1e7), ("a", 1_000_000, 0.0)]));
        assert_eq!(index.search("email", "all", 60, 1e7).unwrap()[0].id, "b");
        index.update_usage(&usage(&[("b", 10, 1e7), ("a", 20, 1e7)]));
        assert_eq!(index.search("email", "all", 60, 1e7).unwrap()[0].id, "a");
        assert_eq!(index.search("eml", "all", 60, 0.0).unwrap(), index.search("eml", "all", 60, 0.0).unwrap());
    }

    #[test]
    fn groups_filters_unicode_and_fuzzy() {
        let index = Index::new(synthetic_records(&synthetic_library(10), 100), &Usage::new());
        let rows = index.search("", "all", 60, 0.0).unwrap();
        assert_eq!(rows[0].id, "clipboard-00099");
        assert!(rows.iter().any(|r| r.kind == Kind::Snippet));
        assert!(index.search("", "snippet", 60, 0.0).unwrap().iter().all(|r| r.kind == Kind::Snippet));
        assert!(index.search("", "clipboard", 60, 0.0).unwrap().iter().all(|r| r.kind == Kind::Clipboard));
        assert_eq!(index.search("!eml", "all", 60, 0.0).unwrap()[0].id, "snippet-0000");
        assert_eq!(index.search("καλη", "all", 60, 0.0).unwrap()[0].id, "snippet-0001");
        assert!(index.search("", "other", 60, 0.0).is_err());
        assert!(index.search(&"x".repeat(257), "all", 60, 0.0).is_err());
    }

    #[test]
    fn full_case_folding_matches_python_casefold() {
        let index = Index::new(
            vec![snippet("case", "Straße", "İstanbul Σ 🌍"), snippet("other", "Other", "unrelated")],
            &Usage::new(),
        );
        for query in ["STRASSE", "i̇st", "σ", "Σ", "🌍", "  straße  "] {
            assert_eq!(ids(index.search(query, "all", 60, 0.0).unwrap()), ["case"], "{query:?}");
        }
    }

    #[test]
    fn index_bounds_payload() {
        let index = Index::new(
            vec![snippet("a", "Email", &("x".repeat(100_000) + "hidden")), snippet("b", "Bound", &("🌍".repeat(510) + "END"))],
            &Usage::new(),
        );
        assert!(index.search("hidden", "all", 60, 0.0).unwrap().is_empty());
        assert_eq!(index.search("xxx", "all", 60, 0.0).unwrap()[0].id, "a");
        assert_eq!(ids(index.search("en", "all", 60, 0.0).unwrap()), ["b"]);
        assert!(index.search("end", "all", 60, 0.0).unwrap().is_empty());
    }

    #[test]
    fn tiers_and_spreads_use_unicode_scalars() {
        assert_eq!(field_match("ab", 2, "ab", 2), Some((0, 0)));
        assert_eq!(field_match("ab", 2, "abc", 3), Some((1, 1)));
        assert_eq!(field_match("ab", 2, "🌍ab", 3), Some((2, 1)));
        assert_eq!(field_match("ac", 2, "🌍a🌍c", 4), Some((3, 1)));
        assert_eq!(field_match("ca", 2, "abc", 3), None);
    }

    #[test]
    fn result_limits() {
        let index = Index::new(synthetic_records(&synthetic_library(200), 200), &Usage::new());
        assert_eq!(index.search("project", "all", 60, 0.0).unwrap().len(), 60);
        let rows = index.search("", "all", 60, 0.0).unwrap();
        assert_eq!(rows.iter().filter(|r| r.kind == Kind::Clipboard).count(), 30);
        assert_eq!(rows.iter().filter(|r| r.kind == Kind::Snippet).count(), 30);
    }
}

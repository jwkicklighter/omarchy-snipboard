//! Pure templates and bounded keyword matching. No desktop access.

use std::collections::BTreeMap;

use unicode_general_category::{GeneralCategory as Category, get_general_category};

use crate::search::{Kind, Record};
use crate::storage::{Library, Snippet};
use crate::sys::LocalTime;
use crate::{Error, Result};

pub const MAX_KEYWORD: usize = 64;
pub const MAX_BODY: usize = 65536;
pub const MAX_CLIPBOARD: usize = 16384;

/// Python's `str.isspace`: Unicode White_Space plus the ASCII information separators.
pub fn is_space(character: char) -> bool {
    character.is_whitespace() || ('\x1c'..='\x1f').contains(&character)
}

pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

fn is_other(character: char) -> bool {
    matches!(
        get_general_category(character),
        Category::Control | Category::Format | Category::Surrogate | Category::PrivateUse | Category::Unassigned
    )
}

/// Letters, numbers, marks, and underscore continue a word; anything else is a boundary.
pub fn word_character(character: char) -> bool {
    use Category::*;
    character == '_'
        || matches!(
            get_general_category(character),
            UppercaseLetter
                | LowercaseLetter
                | TitlecaseLetter
                | ModifierLetter
                | OtherLetter
                | DecimalNumber
                | LetterNumber
                | OtherNumber
                | NonspacingMark
                | SpacingMark
                | EnclosingMark
        )
}

pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

pub fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Empty keywords are allowed; others must be bounded, visible, and neither duplicate
/// nor prefix another (in either order), so typed expansion is never ambiguous.
pub fn validate_keywords<'a>(keywords: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut keywords: Vec<&str> = keywords.into_iter().filter(|keyword| !keyword.is_empty()).collect();
    for keyword in &keywords {
        if char_len(keyword) > MAX_KEYWORD || keyword.chars().any(|c| is_space(c) || is_other(c)) {
            return Err(Error::Invalid("invalid keyword"));
        }
    }
    keywords.sort_unstable();
    if keywords.windows(2).any(|pair| pair[1].starts_with(pair[0])) {
        return Err(Error::Invalid("duplicate or prefix-conflicting keywords"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variable {
    Date,
    Time,
    Clipboard,
    Cursor,
}

const VARIABLES: [(&str, Variable); 4] = [
    ("date", Variable::Date),
    ("time", Variable::Time),
    ("clipboard", Variable::Clipboard),
    ("cursor", Variable::Cursor),
];

/// A `{name}` variable, or an escaped `{{name}}` that renders as literal `{name}`.
/// Offsets are in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub escaped: bool,
    pub variable: Variable,
}

fn token_at(text: &str, start: usize) -> Option<Token> {
    for (open, close, escaped) in [("{{", "}}", true), ("{", "}", false)] {
        let Some(inner) = text[start..].strip_prefix(open) else { continue };
        for (name, variable) in VARIABLES {
            if inner.strip_prefix(name).is_some_and(|rest| rest.starts_with(close)) {
                let end = start + open.len() + name.len() + close.len();
                return Some(Token { start, end, escaped, variable });
            }
        }
    }
    None
}

/// Leftmost, non-overlapping variable tokens.
pub fn tokens(body: &str) -> impl Iterator<Item = Token> + '_ {
    let mut position = 0;
    std::iter::from_fn(move || {
        while let Some(offset) = body[position..].find('{') {
            let start = position + offset;
            if let Some(token) = token_at(body, start) {
                position = token.end;
                return Some(token);
            }
            position = start + 1;
        }
        None
    })
}

pub fn cursor_markers(body: &str) -> usize {
    tokens(body)
        .filter(|token| !token.escaped && token.variable == Variable::Cursor)
        .count()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /// Cursor position in Unicode scalar values, if the template placed one.
    pub cursor: Option<usize>,
}

impl Rendered {
    /// Qt positions text in UTF-16 units; without a marker the cursor goes to the end.
    pub fn utf16_cursor(&self) -> usize {
        match self.cursor {
            Some(cursor) => self.text.chars().take(cursor).map(char::len_utf16).sum(),
            None => utf16_length(&self.text),
        }
    }
}

/// Single pass: clipboard content is inserted literally and never re-expanded.
pub fn render(body: &str, clipboard: &str, now: Option<LocalTime>) -> Result<Rendered> {
    if char_len(body) > MAX_BODY || char_len(clipboard) > MAX_CLIPBOARD {
        return Err(Error::Invalid("template or clipboard exceeds limit"));
    }
    let now = now.unwrap_or_else(LocalTime::now);
    let (mut text, mut length, mut cursor, mut previous) = (String::new(), 0, None, 0);
    for token in tokens(body) {
        let literal = &body[previous..token.start];
        text.push_str(literal);
        length += char_len(literal);
        let name = VARIABLES.iter().find(|(_, v)| *v == token.variable).map(|(n, _)| *n).unwrap_or_default();
        let value = if token.escaped {
            format!("{{{name}}}")
        } else {
            match token.variable {
                Variable::Cursor => {
                    if cursor.is_some() {
                        return Err(Error::Invalid("multiple cursor markers"));
                    }
                    cursor = Some(length);
                    String::new()
                }
                Variable::Date => now.date(),
                Variable::Time => now.time(),
                Variable::Clipboard => clipboard.to_owned(),
            }
        };
        length += char_len(&value);
        text.push_str(&value);
        previous = token.end;
    }
    text.push_str(&body[previous..]);
    if char_len(&text) > MAX_BODY {
        return Err(Error::Invalid("rendered text exceeds limit"));
    }
    Ok(Rendered { text, cursor })
}

/// What the matcher knows about the text it is being fed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// The receiving context is explicitly trusted for expansion.
    pub allowed: bool,
    /// The text was produced by an expansion or other synthetic source.
    pub injected: bool,
    /// The context begins at a word boundary.
    pub boundary: bool,
}

/// Disabled by default, bounded and ephemeral; unknown context is not a boundary.
pub struct Matcher {
    keywords: BTreeMap<String, String>,
    pub enabled: bool,
    pub buffer: String,
    pub context: Option<String>,
    boundary: bool,
}

impl Matcher {
    pub fn new(snippets: &[Snippet]) -> Result<Self> {
        validate_keywords(snippets.iter().map(|s| s.keyword.as_str()))?;
        let keywords = snippets
            .iter()
            .filter(|s| !s.keyword.is_empty())
            .map(|s| (s.keyword.clone(), s.id.clone()))
            .collect();
        Ok(Matcher { keywords, enabled: false, buffer: String::new(), context: None, boundary: false })
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.context = None;
        self.boundary = false;
    }

    pub fn feed(&mut self, text: &str, context: Option<&str>, input: Input) -> Option<String> {
        let Some(context) = context.filter(|_| self.enabled && input.allowed && !input.injected) else {
            self.reset();
            return None;
        };
        if self.context.as_deref() != Some(context) {
            self.reset();
            self.context = Some(context.to_owned());
            self.boundary = input.boundary;
        }
        for character in text.chars() {
            let mut candidate = self.buffer.clone();
            candidate.push(character);
            if self.boundary {
                if let Some(id) = self.keywords.get(&candidate) {
                    let id = id.clone();
                    self.reset();
                    return Some(id);
                }
                if self.keywords.keys().any(|keyword| keyword.starts_with(&candidate)) {
                    self.buffer = candidate.chars().take(MAX_KEYWORD).collect();
                    continue;
                }
            }
            self.buffer.clear();
            self.boundary = !word_character(character);
        }
        None
    }
}

/// Only inspect a bounded prefix of an explicitly trusted scratch TextEdit.
pub fn scratch_match<'a>(prefix: &str, snippets: &'a [Snippet]) -> Option<&'a Snippet> {
    snippets.iter().find(|snippet| {
        !snippet.keyword.is_empty()
            && prefix
                .strip_suffix(snippet.keyword.as_str())
                .is_some_and(|preceding| preceding.chars().next_back().is_none_or(|c| !word_character(c)))
    })
}

pub fn synthetic_library(count: usize) -> Library {
    let special = [
        ("Email", "!email", "scratch@example.invalid"),
        ("Unicode", "!unicode", "Hello — Καλημέρα 🌍"),
        ("Multiline", "!multi", "First line\nSecond line\nThird line"),
        ("Variables", "!vars", "{date} {time}\n{clipboard}\nBefore {cursor}after"),
    ];
    let snippets = (0..count)
        .map(|i| {
            let (name, keyword, body) = match special.get(i) {
                Some(&(name, keyword, body)) => (name.to_owned(), keyword.to_owned(), body.to_owned()),
                None => (
                    format!("Project note {i:04}"),
                    String::new(),
                    format!("Synthetic reusable project note {i:04} amber river"),
                ),
            };
            Snippet {
                id: format!("snippet-{i:04}"),
                name,
                keyword,
                body,
                notes: "Synthetic prototype fixture".to_owned(),
            }
        })
        .collect();
    Library { snippets, ..Library::default() }
}

pub fn synthetic_records(library: &Library, count: usize) -> Vec<Record> {
    let mut records: Vec<Record> = (0..count)
        .map(|i| {
            let mut body = format!("Synthetic clipboard {i:05} amber river project meeting notes");
            if i % 1000 == 0 {
                body.push('\n');
                body.push_str(&"Large synthetic payload. ".repeat(2000));
            }
            Record {
                id: format!("clipboard-{i:05}"),
                kind: Kind::Clipboard,
                name: format!("Clipboard {i:05}"),
                body,
                created: i as f64,
                ..Record::default()
            }
        })
        .collect();
    records.extend(library.snippets.iter().map(Record::from));
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<LocalTime> {
        Some(LocalTime { year, month, day, hour, minute })
    }

    #[test]
    fn template_renders_once_escapes_unknown_and_local_time() {
        let result = render("{date} {time} {clipboard} {unknown} {{date}}", "{time}", at(2026, 10, 15, 9, 7)).unwrap();
        assert_eq!(result.text, "2026-10-15 09:07 {time} {unknown} {date}");
    }

    #[test]
    fn template_unicode_multiline_cursor() {
        let result = render("🌍\n{clipboard}{cursor}end", "é\n", None).unwrap();
        assert_eq!(result.text, "🌍\né\nend");
        assert_eq!(result.cursor, Some(4));
        assert_eq!(result.utf16_cursor(), 5);
        assert!(render("{cursor}{cursor}", "", None).is_err());
        assert_eq!(render("{{cursor}} {cursor}", "", None).unwrap().cursor, Some(9));
    }

    #[test]
    fn template_bounds() {
        assert!(render("{clipboard}", &"a".repeat(16385), None).is_err());
        assert!(render("{clipboard}", &"a".repeat(16384), None).is_ok());
        assert!(render(&"{clipboard}".repeat(5), &"a".repeat(16384), None).is_err());
    }

    #[test]
    fn keyword_conflicts_both_orders_optional_case_sensitive() {
        for keys in [["!a", "!ab"], ["!ab", "!a"], ["!a", "!a"]] {
            assert!(validate_keywords(keys).is_err(), "{keys:?}");
        }
        validate_keywords(["", "", "!a", "!A"]).unwrap();
        for invalid in ["two words", "tab\t", "nul\0", "zero\u{200b}width", &"x".repeat(65)] {
            assert!(validate_keywords([invalid]).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn scratch_boundaries() {
        let snippets = synthetic_library(4).snippets;
        for prefix in ["!email", " !email", "(!email", "\n!email"] {
            assert_eq!(scratch_match(prefix, &snippets).unwrap().id, "snippet-0000");
        }
        for prefix in ["a!email", "_!email", "é!email", "\u{301}!email", "!Email", "!emai"] {
            assert!(scratch_match(prefix, &snippets).is_none(), "{prefix:?}");
        }
    }

    #[test]
    fn matcher_gates_context_switch_and_injection() {
        let mut matcher = Matcher::new(&synthetic_library(4).snippets).unwrap();
        let trusted = Input { allowed: true, boundary: true, ..Input::default() };
        assert_eq!(matcher.feed("!email", Some("scratch/us"), trusted), None);
        matcher.enabled = true;
        assert_eq!(matcher.feed("!em", Some("scratch/dvorak"), trusted), None);
        assert_eq!(matcher.feed("ail", Some("scratch/qwerty"), Input { allowed: true, ..Input::default() }), None);
        assert_eq!(matcher.buffer, "");
        assert_eq!(matcher.feed("!email", Some("scratch/us"), Input { injected: true, ..trusted }), None);
        assert_eq!(matcher.context, None);
        assert_eq!(matcher.feed("!email", Some("scratch/us"), trusted).as_deref(), Some("snippet-0000"));
        for (allowed, context) in [(false, Some("password-manager")), (false, Some("locked")), (false, Some("picker")), (true, None)] {
            assert_eq!(matcher.feed("!email", context, Input { allowed, ..trusted }), None);
            assert_eq!(matcher.buffer, "");
        }
    }

    #[test]
    fn matcher_is_ephemeral_and_bounded() {
        let mut matcher = Matcher::new(&synthetic_library(4).snippets).unwrap();
        matcher.enabled = true;
        let trusted = Input { allowed: true, boundary: true, ..Input::default() };
        matcher.feed(&"x".repeat(100_000), Some("scratch"), trusted);
        assert!(char_len(&matcher.buffer) <= MAX_KEYWORD);
        matcher.reset();
        assert_eq!(matcher.context, None);
    }

    #[test]
    fn python_whitespace_and_word_semantics() {
        assert_eq!(strip("\x1c\u{a0} a b \u{85}"), "a b");
        assert!(word_character('é') && word_character('\u{301}') && word_character('٣') && word_character('_'));
        assert!(!word_character('!') && !word_character(' ') && !word_character('🌍'));
    }
}

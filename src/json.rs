//! Protocol JSON helpers: ASCII-only output and tolerant handling of lone surrogates.

use std::borrow::Cow;
use std::io;

use serde::Serialize;
use serde_json::ser::{Formatter, Serializer};

/// Compact output with every non-ASCII (and DEL) character escaped as UTF-16 `\uXXXX`,
/// so line-oriented readers never split a multibyte sequence.
pub struct AsciiFormatter;

impl Formatter for AsciiFormatter {
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()> {
        let mut start = 0;
        for (index, character) in fragment.char_indices() {
            if (' '..='~').contains(&character) {
                continue;
            }
            writer.write_all(&fragment.as_bytes()[start..index])?;
            let mut units = [0u16; 2];
            for unit in character.encode_utf16(&mut units) {
                write!(writer, "\\u{unit:04x}")?;
            }
            start = index + character.len_utf8();
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }
}

pub fn to_ascii_string<T: Serialize + ?Sized>(value: &T) -> String {
    let mut output = Vec::new();
    value
        .serialize(&mut Serializer::with_formatter(&mut output, AsciiFormatter))
        .expect("serializing JSON values cannot fail");
    String::from_utf8(output).expect("ASCII formatter only emits ASCII")
}

/// JavaScript may send lone UTF-16 surrogate escapes, which strict JSON parsing rejects
/// for the whole line (losing `op`/`seq`, so the UI could not match the error). Replace
/// each with `\u0000`, which every stored text field already rejects.
pub fn replace_lone_surrogates(line: &[u8]) -> Cow<'_, [u8]> {
    fn unit(bytes: &[u8]) -> Option<u16> {
        if bytes.len() < 6 || bytes[0] != b'\\' || bytes[1] != b'u' {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(&bytes[2..6]).ok()?, 16).ok()
    }
    let high = |unit: u16| (0xD800..0xDC00).contains(&unit);
    let low = |unit: u16| (0xDC00..0xE000).contains(&unit);

    let mut output: Option<Vec<u8>> = None;
    let mut index = 0;
    while index < line.len() {
        if line[index] != b'\\' {
            index += 1;
            continue;
        }
        // Backslashes only occur inside strings in valid JSON; every escape is two bytes
        // except `\uXXXX`, so skipping escapes whole keeps `\\u` from being misread.
        let Some(code) = unit(&line[index..]) else {
            index += 2;
            continue;
        };
        if high(code) && unit(&line[index + 6..]).is_some_and(low) {
            index += 12;
            continue;
        }
        if high(code) || low(code) {
            let replaced = output.get_or_insert_with(|| line.to_vec());
            replaced[index + 2..index + 6].copy_from_slice(b"0000");
        }
        index += 6;
    }
    output.map_or(Cow::Borrowed(line), Cow::Owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ascii_output_matches_python_ensure_ascii() {
        let value = json!({"b": "é🌍\x7f\n\"", "a": 1});
        assert_eq!(to_ascii_string(&value), r#"{"a":1,"b":"\u00e9\ud83c\udf0d\u007f\n\""}"#);
    }

    #[test]
    fn only_lone_surrogates_are_replaced() {
        let paired = br#"{"a":"\ud83c\udf0d"}"#;
        assert!(matches!(replace_lone_surrogates(paired), Cow::Borrowed(_)));
        let escaped = br#"{"a":"\\ud800"}"#;
        assert!(matches!(replace_lone_surrogates(escaped), Cow::Borrowed(_)));
        let lone = br#"{"a":"\ud800x","b":"\udc00","c":"\ud800\u0041"}"#;
        assert_eq!(&*replace_lone_surrogates(lone), br#"{"a":"\u0000x","b":"\u0000","c":"\u0000\u0041"}"#);
    }
}

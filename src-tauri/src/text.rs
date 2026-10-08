//! Decoding files for the editor and encoding them back exactly as they were stored: the same
//! encoding, byte order mark and line endings.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// How far into a file to look for the NUL bytes that mark it as binary.
const BINARY_SNIFF_BYTES: usize = 8000;
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16_BE_BOM: &[u8] = &[0xFE, 0xFF];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// ISO-8859-1. Every byte maps to one character, so files in any single-byte encoding
    /// open and save without changing a byte the user did not edit.
    Latin1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    Lf,
    Crlf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Lines end in `\n` only; `line_ending` says what they were.
    pub text: String,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
}

pub fn decode(bytes: &[u8]) -> AppResult<Decoded> {
    let (text, encoding) = if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        (utf8(rest)?, TextEncoding::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(UTF16_LE_BOM) {
        (utf16(rest, u16::from_le_bytes)?, TextEncoding::Utf16Le)
    } else if let Some(rest) = bytes.strip_prefix(UTF16_BE_BOM) {
        (utf16(rest, u16::from_be_bytes)?, TextEncoding::Utf16Be)
    } else if looks_binary(bytes) {
        return Err(AppError::invalid(
            "This looks like a binary file, so it cannot be edited as text",
        ));
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_string(), TextEncoding::Utf8),
            Err(_) => (
                bytes.iter().map(|&byte| char::from(byte)).collect(),
                TextEncoding::Latin1,
            ),
        }
    };
    let line_ending = detect_line_ending(&text);
    let text = if text.contains('\r') {
        normalize_line_endings(&text)
    } else {
        text
    };
    Ok(Decoded {
        text,
        encoding,
        line_ending,
    })
}

pub fn encode(text: &str, encoding: TextEncoding, line_ending: LineEnding) -> AppResult<Vec<u8>> {
    let text = match line_ending {
        LineEnding::Lf => std::borrow::Cow::Borrowed(text),
        LineEnding::Crlf => std::borrow::Cow::Owned(text.replace('\n', "\r\n")),
    };
    Ok(match encoding {
        TextEncoding::Utf8 => text.as_bytes().to_vec(),
        TextEncoding::Utf8Bom => [UTF8_BOM, text.as_bytes()].concat(),
        TextEncoding::Utf16Le => {
            [UTF16_LE_BOM.to_vec(), utf16_bytes(&text, u16::to_le_bytes)].concat()
        }
        TextEncoding::Utf16Be => {
            [UTF16_BE_BOM.to_vec(), utf16_bytes(&text, u16::to_be_bytes)].concat()
        }
        TextEncoding::Latin1 => latin1_bytes(&text)?,
    })
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
}

fn utf8(bytes: &[u8]) -> AppResult<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::invalid("The file is marked as UTF-8 but is not valid UTF-8"))
}

fn utf16(bytes: &[u8], read_unit: fn([u8; 2]) -> u16) -> AppResult<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(AppError::invalid(
            "The file is marked as UTF-16 but has an odd number of bytes",
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| read_unit([pair[0], pair[1]]));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .map_err(|_| AppError::invalid("The file is marked as UTF-16 but is not valid UTF-16"))
}

fn utf16_bytes(text: &str, write_unit: fn(u16) -> [u8; 2]) -> Vec<u8> {
    text.encode_utf16().flat_map(write_unit).collect()
}

fn latin1_bytes(text: &str) -> AppResult<Vec<u8>> {
    text.chars()
        .map(|character| {
            u8::try_from(u32::from(character)).map_err(|_| {
                AppError::invalid(format!(
                    "The character {character} cannot be saved in Latin-1. Switch the encoding to UTF-8 to keep it."
                ))
            })
        })
        .collect()
}

/// The ending most lines use; files with no line break count as LF.
fn detect_line_ending(text: &str) -> LineEnding {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if crlf > lf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

/// Turns `\r\n` and lone `\r` into `\n`.
fn normalize_line_endings(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(character);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(bytes: &[u8]) -> Decoded {
        let decoded = decode(bytes).unwrap();
        let encoded = encode(&decoded.text, decoded.encoding, decoded.line_ending).unwrap();
        assert_eq!(encoded, bytes);
        decoded
    }

    #[test]
    fn utf8_with_lf_round_trips() {
        let decoded = round_trip("server {\n  listen 443;\n}\n".as_bytes());
        assert_eq!(decoded.encoding, TextEncoding::Utf8);
        assert_eq!(decoded.line_ending, LineEnding::Lf);
    }

    #[test]
    fn crlf_is_normalized_and_restored() {
        let decoded = round_trip(b"[core]\r\nname = poros\r\n");
        assert_eq!(decoded.text, "[core]\nname = poros\n");
        assert_eq!(decoded.line_ending, LineEnding::Crlf);
    }

    #[test]
    fn byte_order_marks_are_kept() {
        assert_eq!(
            round_trip(b"\xEF\xBB\xBFhello\n").encoding,
            TextEncoding::Utf8Bom
        );
        let little_endian = [0xFF, 0xFE, b'h', 0, b'i', 0, 0x0A, 0];
        let decoded = round_trip(&little_endian);
        assert_eq!(decoded.encoding, TextEncoding::Utf16Le);
        assert_eq!(decoded.text, "hi\n");
        let big_endian = [0xFE, 0xFF, 0, b'o', 0, b'k'];
        assert_eq!(round_trip(&big_endian).encoding, TextEncoding::Utf16Be);
    }

    #[test]
    fn invalid_utf8_opens_as_latin1_without_losing_bytes() {
        let decoded = round_trip(b"caf\xE9 \x80\xFF\n");
        assert_eq!(decoded.encoding, TextEncoding::Latin1);
        assert_eq!(decoded.text.chars().nth(3), Some('\u{e9}'));
    }

    #[test]
    fn latin1_refuses_characters_it_cannot_store() {
        let error = encode("snow \u{2603}", TextEncoding::Latin1, LineEnding::Lf).unwrap_err();
        assert!(error.message.contains("UTF-8"));
    }

    #[test]
    fn binary_files_are_refused() {
        assert!(decode(b"\x7fELF\x02\x01\x01\x00\x00").is_err());
    }

    #[test]
    fn mixed_endings_follow_the_majority() {
        let decoded = decode(b"one\r\ntwo\r\nthree\nfour\r").unwrap();
        assert_eq!(decoded.line_ending, LineEnding::Crlf);
        assert_eq!(decoded.text, "one\ntwo\nthree\nfour\n");
    }

    #[test]
    fn empty_files_are_utf8_with_lf() {
        let decoded = round_trip(b"");
        assert_eq!(decoded.encoding, TextEncoding::Utf8);
        assert_eq!(decoded.line_ending, LineEnding::Lf);
    }
}

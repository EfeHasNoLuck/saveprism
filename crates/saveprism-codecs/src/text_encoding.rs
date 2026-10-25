//! Text-encoding detection and lossless conversion shared by the text codecs.
//!
//! BOMs are checked first (UTF-8, UTF-16LE/BE, UTF-32LE/BE); UTF-32 is decoded here because
//! `encoding_rs` does not implement it. Without a BOM the bytes are tried strictly as UTF-8,
//! Shift_JIS, GB18030, CP949 (EUC-KR) and Windows-1252, and the candidate with the highest
//! script-aware score wins. A candidate is accepted only when encoding its decoded text gives the
//! original bytes again, so an unchanged file always writes back exactly.
//!
//! Encoding labels and the byte-to-character tables come from the WHATWG Encoding Standard
//! (<https://encoding.spec.whatwg.org/>, `shift_jis`, `gb18030`, `euc-kr`, `windows-1252`,
//! `windows-1251`, `koi8-r`) and are implemented by `encoding_rs` (Apache-2.0 OR MIT). The BOM
//! table is the Unicode FAQ "UTF-8, UTF-16, UTF-32 & BOM"
//! (<https://www.unicode.org/faq/utf_bom.html>). Nothing here executes or evaluates text.

use encoding_rs::{
    EUC_KR, Encoding as RsEncoding, GB18030, KOI8_R, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE,
    WINDOWS_1251, WINDOWS_1252,
};
use saveprism_core::{Error, ErrorCode, Result};

/// A character cannot be represented in the file's encoding.
pub(crate) const TEXT_UNREPRESENTABLE: ErrorCode = ErrorCode::new("SP-TEXT-ENC-001");
/// The byte span cannot be mapped back to the original text.
pub(crate) const TEXT_SPAN: ErrorCode = ErrorCode::new("SP-TEXT-ENC-002");

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];
const UTF32LE_BOM: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
const UTF32BE_BOM: &[u8] = &[0x00, 0x00, 0xFE, 0xFF];

/// A text encoding this crate understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    ShiftJis,
    Gb18030,
    Cp949,
    Windows1252,
    Windows1251,
    Koi8R,
}

/// Decoded text plus the exact encoding used for it.
#[derive(Clone, Debug)]
pub(crate) struct DecodedText {
    pub text: String,
    pub encoding: TextEncoding,
    /// Length of the byte order mark that was stripped from the front.
    pub bom_len: usize,
}

impl TextEncoding {
    /// Stable name stored in codec parameters and shown as file info.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Utf32Le => "utf-32le",
            Self::Utf32Be => "utf-32be",
            Self::ShiftJis => "shift_jis",
            Self::Gb18030 => "gb18030",
            Self::Cp949 => "cp949",
            Self::Windows1252 => "windows-1252",
            Self::Windows1251 => "windows-1251",
            Self::Koi8R => "koi8-r",
        }
    }

    /// Reads a name written by [`TextEncoding::name`] or an EasyRPG `RPG_RT.ini` code page.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "utf-8" | "utf8" | "65001" => Some(Self::Utf8),
            "utf-16le" | "utf-16" | "utf16le" | "1200" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" | "1201" => Some(Self::Utf16Be),
            "utf-32le" | "utf32le" | "12000" => Some(Self::Utf32Le),
            "utf-32be" | "utf32be" | "12001" => Some(Self::Utf32Be),
            "shift_jis" | "shift-jis" | "sjis" | "windows-31j" | "932" => Some(Self::ShiftJis),
            "gb18030" | "54936" => Some(Self::Gb18030),
            "cp949" | "euc-kr" | "euc_kr" | "windows-949" | "949" => Some(Self::Cp949),
            "windows-1252" | "cp1252" | "1252" => Some(Self::Windows1252),
            "windows-1251" | "cp1251" | "1251" => Some(Self::Windows1251),
            "koi8-r" | "koi8r" | "20866" => Some(Self::Koi8R),
            _ => None,
        }
    }

    /// The byte order mark for this encoding, if it is a Unicode form with one.
    pub(crate) fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8 => UTF8_BOM,
            Self::Utf16Le => UTF16LE_BOM,
            Self::Utf16Be => UTF16BE_BOM,
            Self::Utf32Le => UTF32LE_BOM,
            Self::Utf32Be => UTF32BE_BOM,
            _ => &[],
        }
    }

    fn rs(self) -> Option<&'static RsEncoding> {
        match self {
            Self::Utf8 => Some(UTF_8),
            Self::Utf16Le => Some(UTF_16LE),
            Self::Utf16Be => Some(UTF_16BE),
            Self::ShiftJis => Some(SHIFT_JIS),
            Self::Gb18030 => Some(GB18030),
            Self::Cp949 => Some(EUC_KR),
            Self::Windows1252 => Some(WINDOWS_1252),
            Self::Windows1251 => Some(WINDOWS_1251),
            Self::Koi8R => Some(KOI8_R),
            Self::Utf32Le | Self::Utf32Be => None,
        }
    }

    /// Decodes bytes strictly (no replacement) without BOM handling.
    pub(crate) fn decode(self, bytes: &[u8]) -> Option<String> {
        match self {
            Self::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_owned),
            Self::Utf16Le => decode_utf16(bytes, true),
            Self::Utf16Be => decode_utf16(bytes, false),
            Self::Utf32Le => decode_utf32(bytes, true),
            Self::Utf32Be => decode_utf32(bytes, false),
            _ => {
                let encoding = self.rs()?;
                encoding
                    .decode_without_bom_handling_and_without_replacement(bytes)
                    .map(std::borrow::Cow::into_owned)
            }
        }
    }

    /// Encodes text strictly. Characters the encoding cannot represent are refused with the
    /// Unicode scalar value in the message, never silently replaced.
    pub(crate) fn encode(self, text: &str) -> Result<Vec<u8>> {
        match self {
            Self::Utf8 => Ok(text.as_bytes().to_vec()),
            Self::Utf16Le => Ok(encode_utf16(text, true)),
            Self::Utf16Be => Ok(encode_utf16(text, false)),
            Self::Utf32Le => Ok(encode_utf32(text, true)),
            Self::Utf32Be => Ok(encode_utf32(text, false)),
            _ => {
                let encoding = self
                    .rs()
                    .ok_or_else(|| Error::new(TEXT_UNREPRESENTABLE, "unknown text encoding"))?;
                let mut encoder = encoding.new_encoder();
                let capacity = encoder
                    .max_buffer_length_from_utf8_without_replacement(text.len())
                    .ok_or_else(|| {
                        Error::new(TEXT_UNREPRESENTABLE, "text is too large to encode")
                    })?;
                let mut out = vec![0_u8; capacity];
                let (result, _read, written) =
                    encoder.encode_from_utf8_without_replacement(text, &mut out, true);
                match result {
                    encoding_rs::EncoderResult::InputEmpty => {
                        out.truncate(written);
                        Ok(out)
                    }
                    encoding_rs::EncoderResult::Unmappable(character) => {
                        Err(unrepresentable(self, character))
                    }
                    encoding_rs::EncoderResult::OutputFull => Err(Error::new(
                        TEXT_UNREPRESENTABLE,
                        "the text grows beyond the encoder buffer",
                    )),
                }
            }
        }
    }
}

fn unrepresentable(encoding: TextEncoding, character: char) -> Error {
    Error::new(
        TEXT_UNREPRESENTABLE,
        format!(
            "the character U+{:04X} ({character}) cannot be written in {}",
            u32::from(character),
            encoding.name()
        ),
    )
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        units.push(if little_endian {
            u16::from_le_bytes(*pair)
        } else {
            u16::from_be_bytes(*pair)
        });
    }
    String::from_utf16(&units).ok()
}

fn encode_utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len().saturating_mul(2));
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

fn decode_utf32(bytes: &[u8], little_endian: bool) -> Option<String> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut text = String::with_capacity(bytes.len() / 4);
    for word in bytes.as_chunks::<4>().0 {
        let value = if little_endian {
            u32::from_le_bytes(*word)
        } else {
            u32::from_be_bytes(*word)
        };
        text.push(char::from_u32(value)?);
    }
    Some(text)
}

fn encode_utf32(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len().saturating_mul(4));
    for character in text.chars() {
        let value = u32::from(character);
        let bytes = if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

/// Encodes `text` with `encoding`, adding its BOM when `bom` is true.
pub(crate) fn encode_with(text: &str, encoding: TextEncoding, bom: bool) -> Result<Vec<u8>> {
    let out = encoding.encode(text)?;
    if bom {
        let mark = encoding.bom();
        if !mark.is_empty() {
            let mut prefixed = Vec::with_capacity(mark.len().saturating_add(out.len()));
            prefixed.extend_from_slice(mark);
            prefixed.extend_from_slice(&out);
            return Ok(prefixed);
        }
    }
    Ok(out)
}

/// Detects a text encoding by content and returns the decoded text (BOM stripped).
pub(crate) fn decode_sniff(bytes: &[u8]) -> Option<DecodedText> {
    for (encoding, bom) in [
        (TextEncoding::Utf32Le, UTF32LE_BOM),
        (TextEncoding::Utf32Be, UTF32BE_BOM),
        (TextEncoding::Utf8, UTF8_BOM),
        (TextEncoding::Utf16Le, UTF16LE_BOM),
        (TextEncoding::Utf16Be, UTF16BE_BOM),
    ] {
        if let Some(body) = bytes.strip_prefix(bom)
            && let Some(text) = encoding.decode(body)
        {
            return Some(DecodedText {
                text,
                encoding,
                bom_len: bom.len(),
            });
        }
    }

    if let Some(text) = TextEncoding::Utf8.decode(bytes)
        && is_text_like(&text)
    {
        return Some(DecodedText {
            text,
            encoding: TextEncoding::Utf8,
            bom_len: 0,
        });
    }

    let mut best: Option<(i32, TextEncoding, String)> = None;
    for encoding in [
        TextEncoding::ShiftJis,
        TextEncoding::Gb18030,
        TextEncoding::Cp949,
        TextEncoding::Windows1252,
    ] {
        let Some(text) = encoding.decode(bytes) else {
            continue;
        };
        if !is_text_like(&text) {
            continue;
        }
        // Only an encoding that writes the same bytes back may be used, otherwise the round trip
        // would change an untouched file.
        if encoding.encode(&text).ok().as_deref() != Some(bytes) {
            continue;
        }
        let score = score(encoding, bytes, &text);
        if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
            best = Some((score, encoding, text));
        }
    }
    best.map(|(_, encoding, text)| DecodedText {
        text,
        encoding,
        bom_len: 0,
    })
}

/// Decodes `bytes` as one of the legacy Cyrillic code pages and picks the more plausible one.
pub(crate) fn guess_cyrillic(bytes: &[u8]) -> TextEncoding {
    let score_of = |encoding: TextEncoding| {
        encoding
            .decode(bytes)
            .filter(|text| is_text_like(text))
            .map_or(-100, |text| {
                if encoding.encode(&text).ok().as_deref() == Some(bytes) {
                    cyrillic_score(&text)
                } else {
                    -100
                }
            })
    };
    if score_of(TextEncoding::Koi8R) > score_of(TextEncoding::Windows1251) {
        TextEncoding::Koi8R
    } else {
        TextEncoding::Windows1251
    }
}

fn is_text_like(text: &str) -> bool {
    !text.chars().any(|character| {
        character.is_control() && !matches!(character, '\t' | '\n' | '\r' | '\u{0C}')
    })
}

fn score(encoding: TextEncoding, bytes: &[u8], text: &str) -> i32 {
    let mut kana = 0_i32;
    let mut hangul = 0_i32;
    let mut common_hangul = 0_i32;
    let mut cjk = 0_i32;
    let mut cyrillic = 0_i32;
    let mut latin = 0_i32;
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        match u32::from(*character) {
            0x3040..=0x30FF => kana += 1,
            0xAC00..=0xD7A3 | 0x1100..=0x11FF => {
                hangul += 1;
                if COMMON_HANGUL.contains(*character) {
                    common_hangul += 1;
                }
            }
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => cjk += 1,
            0x0400..=0x04FF => cyrillic += 1,
            0x00C0..=0x024F if character.is_alphabetic() => {
                // Western text puts accented letters inside words (`Zürich`); a misread
                // multi-byte stream produces clusters of accented letters instead.
                let in_word = index
                    .checked_sub(1)
                    .and_then(|before| characters.get(before))
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
                    || characters
                        .get(index.saturating_add(1))
                        .is_some_and(|ch| ch.is_ascii_alphanumeric());
                if in_word {
                    latin += 1;
                }
            }
            _ => {}
        }
    }
    let four_byte = bytes
        .windows(4)
        .filter(|window| {
            window
                .first()
                .is_some_and(|first| (0x81..=0xFE).contains(first))
                && window
                    .get(1)
                    .is_some_and(|second| (0x30..=0x39).contains(second))
                && window
                    .get(2)
                    .is_some_and(|third| (0x81..=0xFE).contains(third))
                && window
                    .get(3)
                    .is_some_and(|fourth| (0x30..=0x39).contains(fourth))
        })
        .count() as i32;
    match encoding {
        TextEncoding::ShiftJis => kana.saturating_mul(12) + cjk.saturating_mul(2),
        TextEncoding::Gb18030 => cjk.saturating_mul(4) + four_byte.saturating_mul(6) + latin,
        TextEncoding::Cp949 => {
            hangul.saturating_mul(2) + common_hangul.saturating_mul(8) + cjk.saturating_mul(2)
        }
        TextEncoding::Windows1252 => latin.saturating_mul(6),
        TextEncoding::Windows1251 | TextEncoding::Koi8R => cyrillic,
        _ => 0,
    }
}

/// Frequent modern Korean syllables. A Chinese or Japanese byte stream misread as EUC-KR lands on
/// rare syllables, while real Korean text is made mostly of these.
const COMMON_HANGUL: &str = "가각간갈감갑강개거건걸검게겨격견결경계고곡곤골공과관광교구국군굴궁권귀규균그극근글금급기긴길김나난날남내너널네녀년노논놀농누눈능니다단달담당대더덕도독돈돌동두드득든들등디라락란람랑래량러레력련렬령로록론롱료루류리린림마막만말망매머먹메며면명모목몰무문물미민바박반발방배백버번벌범법변별병보복본볼봉부북분불비빈사삭산살삼상새색생서석선설성세소속손송수숙순술스슬습승시식신실심십아악안알암앙애야약양어억언얼엄업에여역연열영예오옥온올와완왕외요용우운울원월위유육윤율은을음응의이익인일임자작잔잠장재저적전절점정제조족존종주죽준줄중증지직진질집차착찬참창채처천철첨청체초총최추축출충취치친칠카커코크키타탄탈탐탑태터토통투특파판팔패편평포표품풍피필하학한할함합항해행향허헌현혈형혜호혹혼홍화확환활황회효후훈휘휴흑흔흥희히";

/// A crude Russian-letter score; the wrong code page turns common letters into rare ones.
fn cyrillic_score(text: &str) -> i32 {
    let mut score = 0_i32;
    for character in text.chars() {
        score += match character.to_ascii_lowercase() {
            'о' | 'е' | 'а' | 'и' | 'н' | 'т' | 'с' | 'р' | 'в' | 'л' | 'к' | 'м' | 'д' | 'п'
            | 'у' | 'я' => 3,
            'г' | 'з' | 'б' | 'ч' | 'й' | 'х' | 'ж' | 'ш' | 'ю' | 'ц' | 'щ' | 'ф' | 'ы' => {
                1
            }
            character if ('\u{0400}'..='\u{04FF}').contains(&character) => 0,
            _ => 0,
        };
    }
    score
}

/// A patch in UTF-8 text coordinates: byte range and replacement text.
pub(crate) type TextPatch = (usize, usize, Vec<u8>);

/// Applies patches expressed over the decoded UTF-8 `text` to the original `bytes`, translating
/// every span back to the file's encoding. Overlapping spans are refused.
pub(crate) fn apply_text_patches(
    original: &[u8],
    decoded: &DecodedText,
    code: ErrorCode,
    patches: &mut Vec<TextPatch>,
) -> Result<Vec<u8>> {
    patches.sort_by_key(|(start, _, _)| *start);
    for pair in patches.windows(2) {
        if let (Some(first), Some(second)) = (pair.first(), pair.get(1))
            && first.1 > second.0
        {
            return Err(Error::new(code, "overlapping token spans"));
        }
    }

    let text = decoded.text.as_str();
    let mut out = Vec::with_capacity(original.len());
    let mut text_cursor = 0usize;
    let mut byte_cursor = decoded.bom_len;
    let mut copy_from = 0usize;
    for (start, end, replacement) in patches.drain(..) {
        if start > end || end > text.len() {
            return Err(Error::new(code, "token span outside the original text"));
        }
        let before = text.get(text_cursor..start).ok_or_else(|| {
            Error::new(
                TEXT_SPAN,
                format!(
                    "token span {start}..{end} is not on a character boundary (cursor {text_cursor}, text {} bytes)",
                    text.len()
                ),
            )
        })?;
        let before_bytes = decoded.encoding.encode(before)?;
        let byte_start = byte_cursor.saturating_add(before_bytes.len());
        let old = text.get(start..end).ok_or_else(|| {
            Error::new(
                TEXT_SPAN,
                format!(
                    "token span {start}..{end} is not on a character boundary (text {} bytes)",
                    text.len()
                ),
            )
        })?;
        let old_bytes = decoded.encoding.encode(old)?;
        let byte_end = byte_start.saturating_add(old_bytes.len());
        let source = original
            .get(byte_start..byte_end)
            .ok_or_else(|| Error::new(code, "token span outside the original text"))?;
        if source != old_bytes.as_slice() {
            return Err(Error::new(
                TEXT_SPAN,
                "the original text does not match the detected encoding",
            ));
        }
        let rendered = String::from_utf8(replacement)
            .map_err(|_| Error::new(code, "the replacement text is not UTF-8"))?;
        let encoded = decoded.encoding.encode(&rendered)?;
        out.extend_from_slice(
            original
                .get(copy_from..byte_start)
                .ok_or_else(|| Error::new(code, "token span outside the original text"))?,
        );
        out.extend_from_slice(&encoded);
        copy_from = byte_end;
        byte_cursor = byte_end;
        text_cursor = end;
    }
    out.extend_from_slice(
        original
            .get(copy_from..)
            .ok_or_else(|| Error::new(code, "token span outside the original text"))?,
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_forms_are_detected_and_round_trip() {
        for (encoding, text) in [
            (TextEncoding::Utf8, "{\"name\":\"Mira\"}"),
            (TextEncoding::Utf16Le, "{\"name\":\"Mira\"}"),
            (TextEncoding::Utf16Be, "{\"name\":\"Mira\"}"),
            (TextEncoding::Utf32Le, "{\"name\":\"Mira\"}"),
            (TextEncoding::Utf32Be, "{\"name\":\"Mira\"}"),
        ] {
            let bytes = encode_with(text, encoding, true).expect("encodes");
            let decoded = decode_sniff(&bytes).expect("detects");
            assert_eq!(decoded.encoding, encoding);
            assert_eq!(decoded.text, text);
            assert_eq!(decoded.bom_len, encoding.bom().len());
            assert_eq!(encode_with(&decoded.text, encoding, true).unwrap(), bytes);
        }
    }

    #[test]
    fn legacy_encodings_round_trip_and_are_told_apart() {
        for (encoding, text) in [
            (
                TextEncoding::ShiftJis,
                "{\"name\":\"ミラ\",\"city\":\"東京\"}",
            ),
            (
                TextEncoding::Gb18030,
                "{\"name\":\"米拉\",\"city\":\"北京\"}",
            ),
            (TextEncoding::Cp949, "{\"name\":\"미라\",\"city\":\"서울\"}"),
            (
                TextEncoding::Windows1252,
                "{\"name\":\"Mira\",\"city\":\"Zürich\"}",
            ),
        ] {
            let bytes = encoding.encode(text).expect("encodes");
            let decoded = decode_sniff(&bytes).expect("detects");
            assert_eq!(decoded.encoding, encoding, "{text}");
            assert_eq!(decoded.text, text);
            assert_eq!(decoded.encoding.encode(&decoded.text).unwrap(), bytes);
        }
    }

    #[test]
    fn a_character_the_encoding_cannot_hold_is_refused() {
        let error = TextEncoding::Windows1252
            .encode("ミラ")
            .expect_err("must be refused");
        assert_eq!(error.code(), TEXT_UNREPRESENTABLE);
        assert!(error.to_string().contains("U+30DF"));
    }

    #[test]
    fn patches_map_through_the_encoding() {
        let text = "{\"name\":\"ミラ\"}";
        let bytes = TextEncoding::ShiftJis.encode(text).unwrap();
        let decoded = DecodedText {
            text: text.to_owned(),
            encoding: TextEncoding::ShiftJis,
            bom_len: 0,
        };
        let start = text.find("ミラ").unwrap();
        let mut patches = vec![(start, start + "ミラ".len(), "アキ".as_bytes().to_vec())];
        let out = apply_text_patches(&bytes, &decoded, TEXT_SPAN, &mut patches).unwrap();
        assert_eq!(
            TextEncoding::ShiftJis.decode(&out).unwrap(),
            "{\"name\":\"アキ\"}"
        );
    }
}

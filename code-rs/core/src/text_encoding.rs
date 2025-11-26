//! Text encoding detection and conversion utilities for shell output.
//!
//! Windows users frequently run into code pages such as CP1251 or CP866 when invoking commands
//! through VS Code. Those bytes show up as invalid UTF-8 and used to be replaced with the standard
//! Unicode replacement character. We now lean on `chardetng` and `encoding_rs` so we can
//! automatically detect and decode the vast majority of legacy encodings before falling back to
//! lossy UTF-8 decoding.

use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use encoding_rs::GB18030;
use encoding_rs::IBM866;
use encoding_rs::ISO_8859_3;
use encoding_rs::WINDOWS_1250;
use encoding_rs::WINDOWS_1252;
use encoding_rs::WINDOWS_1254;
use encoding_rs::WINDOWS_1256;
use encoding_rs::WINDOWS_874;

/// Attempts to convert arbitrary bytes to UTF-8 with best-effort encoding detection.
pub fn bytes_to_string_smart(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(utf8_str) = std::str::from_utf8(bytes) {
        return utf8_str.to_owned();
    }

    let encoding = detect_encoding(bytes);
    choose_best_decoding(bytes, encoding)
}

// Windows-1252 reassigns a handful of 0x80-0x9F slots to smart punctuation (curly quotes, dashes,
// ™). CP866 uses those *same byte values* for uppercase Cyrillic letters. When chardetng sees shell
// snippets that mix these bytes with ASCII it sometimes guesses IBM866, so “smart quotes” render as
// Cyrillic garbage (“УФЦ”) in VS Code. However, CP866 uppercase tokens are perfectly valid output
// (e.g., `ПРИ test`) so we cannot flip every 0x80-0x9F byte to Windows-1252 either. The compromise
// is to only coerce IBM866 to Windows-1252 when (a) the high bytes are exclusively the punctuation
// values listed below and (b) we spot adjacent ASCII. This targets the real failure case without
// clobbering legitimate Cyrillic text. If another code page has a similar collision, introduce a
// dedicated allowlist (like this one) plus unit tests that capture the actual shell output we want
// to preserve. Windows-1252 byte values for smart punctuation.
const WINDOWS_1252_PUNCT_BYTES: [u8; 8] = [
    0x91, // ‘ (left single quotation mark)
    0x92, // ’ (right single quotation mark)
    0x93, // “ (left double quotation mark)
    0x94, // ” (right double quotation mark)
    0x95, // • (bullet)
    0x96, // – (en dash)
    0x97, // — (em dash)
    0x99, // ™ (trade mark sign)
];

fn detect_encoding(bytes: &[u8]) -> &'static Encoding {
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let (encoding, _is_confident) = detector.guess_assess(None, true);

    // chardetng occasionally reports IBM866 for short strings that only contain Windows-1252 “smart
    // punctuation” bytes (0x80-0x9F) because that range maps to Cyrillic letters in IBM866. When
    // those bytes show up alongside an ASCII word (typical shell output: `"“`test), we know the
    // intent was likely CP1252 quotes/dashes. Prefer WINDOWS_1252 in that specific situation so we
    // render the characters users expect instead of Cyrillic junk. References:
    // - Windows-1252 reserving 0x80-0x9F for curly quotes/dashes:
    //   https://en.wikipedia.org/wiki/Windows-1252
    // - CP866 mapping 0x93/0x94/0x96 to Cyrillic letters, so the same bytes show up as “УФЦ” when
    //   mis-decoded: https://www.unicode.org/Public/MAPPINGS/VENDORS/MICSFT/PC/CP866.TXT
    if encoding == IBM866 && looks_like_windows_1252_punctuation(bytes) {
        return WINDOWS_1252;
    }

    encoding
}

fn choose_best_decoding(bytes: &[u8], detected: &'static Encoding) -> String {
    // Very short, non-ASCII payloads are often junk rather than a real code page; in those cases
    // prefer the lossy UTF-8 fallback instead of over-confident guesses (e.g., IBM866).
    if bytes.len() <= 3 && bytes.iter().all(|b| *b >= 0x80) && !bytes.iter().any(|b| b.is_ascii()) {
        return decode_lossy(bytes);
    }

    let utf8_lossy = decode_lossy(bytes);
    let ascii_bytes = bytes.iter().filter(|b| b.is_ascii()).count();

    // Candidate order matters: start from the detector guess, then try targeted legacy code pages
    // that commonly show up in shells and are easy to confuse on short inputs.
    const CANDIDATES: [&Encoding; 8] = [
        &WINDOWS_874,
        &WINDOWS_1250,
        &WINDOWS_1254,
        &WINDOWS_1256,
        &WINDOWS_1252,
        &GB18030,
        &ISO_8859_3,
        &IBM866,
    ];

    let mut encodings: Vec<&Encoding> = vec![detected];
    for enc in CANDIDATES {
        if !encodings.iter().any(|&e| e == enc) {
            encodings.push(enc);
        }
    }

    let mut best_score = score_decoded(&utf8_lossy, encoding_rs::UTF_8, detected, ascii_bytes);
    let mut best_text = utf8_lossy;

    for enc in encodings {
        if let Some(decoded) = enc.decode_without_bom_handling_and_without_replacement(bytes) {
            let owned = decoded.into_owned();
            let score = score_decoded(&owned, enc, detected, ascii_bytes);
            if score > best_score {
                best_score = score;
                best_text = owned;
            }
        }
    }

    best_text
}

fn score_decoded(
    text: &str,
    candidate: &'static Encoding,
    detected: &'static Encoding,
    ascii_byte_count: usize,
) -> i32 {
    let mut ascii_char_count = 0usize;
    let mut replacement_count = 0usize;
    let mut latin = 0usize;
    let mut cyrillic = 0usize;
    let mut arabic = 0usize;
    let mut han = 0usize;
    let mut thai = 0usize;
    let mut greek = 0usize;
    let mut hebrew = 0usize;

    let mut char_len = 0usize;

    for ch in text.chars() {
        char_len += 1;
        if ch == '\u{FFFD}' {
            replacement_count += 1;
            continue;
        }
        if ch.is_ascii() {
            if ch.is_ascii_alphabetic() {
                latin += 1;
            }
            ascii_char_count += 1;
            continue;
        }

        match script_tag(ch) {
            Script::Latin => latin += 1,
            Script::Cyrillic => cyrillic += 1,
            Script::Arabic => arabic += 1,
            Script::Han => han += 1,
            Script::Thai => thai += 1,
            Script::Greek => greek += 1,
            Script::Hebrew => hebrew += 1,
            Script::Other => {}
        }
    }

    let ascii_penalty = ascii_byte_count.saturating_sub(ascii_char_count) as i32 * 5;
    let mut score = 0i32;
    score -= ascii_penalty;
    score -= (replacement_count as i32) * 6;

    let primary = *[latin, cyrillic, arabic, han, thai, greek, hebrew]
        .iter()
        .max()
        .unwrap_or(&0) as i32;
    score += primary * 2;

    if candidate == detected {
        score += 1;
    }

    // Encoding-specific boosts to break ties.
    if candidate == GB18030 {
        score += han as i32 * 4;
        if han > 0 && ascii_byte_count == 0 && char_len <= 4 {
            // Small, entirely non-ASCII blobs are more likely to be GBK/GB18030 than Thai when we
            // successfully produce CJK ideographs.
            score += 15;
        }
    }
    if candidate == WINDOWS_874 {
        score += thai as i32 * 4;
    }
    if candidate == WINDOWS_1256 {
        score += arabic as i32 * 3;
    }
    if candidate == WINDOWS_1250 || candidate == ISO_8859_3 {
        score += central_euro_count(text) as i32 * 8;
    }
    if candidate == WINDOWS_1254 {
        score += turkish_letter_count(text) as i32 * 6;
    }

    score
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Cyrillic,
    Arabic,
    Han,
    Thai,
    Greek,
    Hebrew,
    Other,
}

fn script_tag(ch: char) -> Script {
    match ch {
        'a'..='z' | 'A'..='Z' => Script::Latin,
        '\u{00C0}'..='\u{024F}' => Script::Latin,
        '\u{0370}'..='\u{03FF}' => Script::Greek,
        '\u{0400}'..='\u{052F}' => Script::Cyrillic,
        '\u{0590}'..='\u{05FF}' => Script::Hebrew,
        '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' => Script::Arabic,
        '\u{0E00}'..='\u{0E7F}' => Script::Thai,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{20000}'..='\u{2EBEF}' => {
            Script::Han
        }
        _ => Script::Other,
    }
}

fn central_euro_count(text: &str) -> usize {
    const CENTRAL: &[char] = &[
        'á', 'č', 'ď', 'é', 'ě', 'í', 'ň', 'ó', 'ř', 'š', 'ť', 'ú', 'ů', 'ý', 'ž', 'Á', 'Č', 'Ď',
        'É', 'Ě', 'Í', 'Ň', 'Ó', 'Ř', 'Š', 'Ť', 'Ú', 'Ů', 'Ý', 'Ž', 'Ħ', 'ħ',
    ];
    text.chars().filter(|c| CENTRAL.contains(c)).count()
}

fn turkish_letter_count(text: &str) -> usize {
    const TURKISH: &[char] = &['ğ', 'Ğ', 'ş', 'Ş', 'ı', 'İ', 'ç', 'Ç', 'ö', 'Ö', 'ü', 'Ü'];
    text.chars().filter(|c| TURKISH.contains(c)).count()
}

fn decode_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Detect whether the byte stream looks like Windows-1252 “smart punctuation” wrapped around
/// otherwise-ASCII text.
///
/// Context: IBM866 and Windows-1252 share the 0x80-0x9F slot range. In IBM866 these bytes decode to
/// Cyrillic letters, whereas Windows-1252 maps them to curly quotes and dashes. chardetng can guess
/// IBM866 for short snippets that only contain those bytes, which turns shell output such as
/// `“test”` into unreadable Cyrillic. To avoid that, we treat inputs comprising a handful of bytes
/// from the problematic range plus ASCII letters as CP1252 punctuation. We deliberately do *not*
/// cap how many of those punctuation bytes we accept: VS Code frequently prints several quoted
/// phrases (e.g., `"foo" – "bar"`), and truncating the count would once again mis-decode those as
/// Cyrillic. If we discover additional encodings with overlapping byte ranges, prefer adding
/// encoding-specific byte allowlists like `WINDOWS_1252_PUNCT` and tests that exercise real-world
/// shell snippets.
fn looks_like_windows_1252_punctuation(bytes: &[u8]) -> bool {
    let mut saw_extended_punctuation = false;
    let mut saw_ascii_word = false;

    for &byte in bytes {
        if byte >= 0xA0 {
            return false;
        }
        if (0x80..=0x9F).contains(&byte) {
            if !is_windows_1252_punct(byte) {
                return false;
            }
            saw_extended_punctuation = true;
        }
        if byte.is_ascii_alphabetic() {
            saw_ascii_word = true;
        }
    }

    saw_extended_punctuation && saw_ascii_word
}

fn is_windows_1252_punct(byte: u8) -> bool {
    WINDOWS_1252_PUNCT_BYTES.contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::BIG5;
    use encoding_rs::EUC_KR;
    use encoding_rs::GBK;
    use encoding_rs::ISO_8859_2;
    use encoding_rs::ISO_8859_3;
    use encoding_rs::ISO_8859_4;
    use encoding_rs::ISO_8859_5;
    use encoding_rs::ISO_8859_6;
    use encoding_rs::ISO_8859_7;
    use encoding_rs::ISO_8859_8;
    use encoding_rs::ISO_8859_10;
    use encoding_rs::ISO_8859_13;
    use encoding_rs::SHIFT_JIS;
    use encoding_rs::WINDOWS_874;
    use encoding_rs::WINDOWS_1250;
    use encoding_rs::WINDOWS_1251;
    use encoding_rs::WINDOWS_1253;
    use encoding_rs::WINDOWS_1254;
    use encoding_rs::WINDOWS_1255;
    use encoding_rs::WINDOWS_1256;
    use encoding_rs::WINDOWS_1257;
    use encoding_rs::WINDOWS_1258;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_utf8_passthrough() {
        // Fast path: when UTF-8 is valid we should avoid copies and return as-is.
        let utf8_text = "Hello, мир! 世界";
        let bytes = utf8_text.as_bytes();
        assert_eq!(bytes_to_string_smart(bytes), utf8_text);
    }

    #[test]
    fn test_cp1251_russian_text() {
        // Cyrillic text emitted by PowerShell/WSL in CP1251 should decode cleanly.
        let bytes = b"\xEF\xF0\xE8\xEC\xE5\xF0"; // "пример" encoded with Windows-1251
        assert_eq!(bytes_to_string_smart(bytes), "пример");
    }

    #[test]
    fn test_cp1251_privet_word() {
        // Regression: CP1251 words like "Привет" must not be mis-identified as Windows-1252.
        let bytes = b"\xCF\xF0\xE8\xE2\xE5\xF2"; // "Привет" encoded with Windows-1251
        assert_eq!(bytes_to_string_smart(bytes), "Привет");
    }

    #[test]
    fn test_koi8_r_privet_word() {
        // KOI8-R output should decode to the original Cyrillic as well.
        let bytes = b"\xF0\xD2\xC9\xD7\xC5\xD4"; // "Привет" encoded with KOI8-R
        assert_eq!(bytes_to_string_smart(bytes), "Привет");
    }

    #[test]
    fn test_cp866_russian_text() {
        // Legacy consoles (cmd.exe) commonly emit CP866 bytes for Cyrillic content.
        let bytes = b"\xAF\xE0\xA8\xAC\xA5\xE0"; // "пример" encoded with CP866
        assert_eq!(bytes_to_string_smart(bytes), "пример");
    }

    #[test]
    fn test_cp866_uppercase_text() {
        // Ensure the IBM866 heuristic still returns IBM866 for uppercase-only words.
        let bytes = b"\x8F\x90\x88"; // "ПРИ" encoded with CP866 uppercase letters
        assert_eq!(bytes_to_string_smart(bytes), "ПРИ");
    }

    #[test]
    fn test_cp866_uppercase_followed_by_ascii() {
        // Regression test: uppercase CP866 tokens next to ASCII text should not be treated as
        // CP1252.
        let bytes = b"\x8F\x90\x88 test"; // "ПРИ test" encoded with CP866 uppercase letters followed by ASCII
        assert_eq!(bytes_to_string_smart(bytes), "ПРИ test");
    }

    #[test]
    fn test_windows_1252_quotes() {
        // Smart detection should map Windows-1252 punctuation into proper Unicode.
        let bytes = b"\x93\x94test";
        assert_eq!(bytes_to_string_smart(bytes), "\u{201C}\u{201D}test");
    }

    #[test]
    fn test_windows_1252_multiple_quotes() {
        // Longer snippets of punctuation (e.g., “foo” – “bar”) should still flip to CP1252.
        let bytes = b"\x93foo\x94 \x96 \x93bar\x94";
        assert_eq!(
            bytes_to_string_smart(bytes),
            "\u{201C}foo\u{201D} \u{2013} \u{201C}bar\u{201D}"
        );
    }

    #[test]
    fn test_windows_1252_privet_gibberish_is_preserved() {
        // Windows-1252 cannot encode Cyrillic; if the input literally contains "ÐŸÑ..." we should not "fix" it.
        let bytes = "ÐŸÑ€Ð¸Ð²ÐµÑ‚".as_bytes();
        assert_eq!(bytes_to_string_smart(bytes), "ÐŸÑ€Ð¸Ð²ÐµÑ‚");
    }

    #[test]
    fn test_iso8859_1_latin_text() {
        // ISO-8859-1 (code page 28591) is the Latin segment used by LatArCyrHeb.
        // encoding_rs unifies ISO-8859-1 with Windows-1252, so reuse that constant here.
        let (encoded, _, had_errors) = WINDOWS_1252.encode("Hello");
        assert!(!had_errors, "failed to encode Latin sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), "Hello");
    }

    #[test]
    fn test_iso8859_2_central_european_text() {
        // ISO-8859-2 (code page 28592) covers additional Central European glyphs.
        let (encoded, _, had_errors) = ISO_8859_2.encode("Příliš žluťoučký kůň");
        assert!(!had_errors, "failed to encode ISO-8859-2 sample");
        assert_eq!(
            bytes_to_string_smart(encoded.as_ref()),
            "Příliš žluťoučký kůň"
        );
    }

    #[test]
    fn test_iso8859_3_southern_european_text() {
        // ISO-8859-3 (code page 28593) covers Southern European languages.
        let sample = "Ħelow";
        let (encoded, _, had_errors) = ISO_8859_3.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-3 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_4_northern_european_text() {
        // ISO-8859-4 (code page 28594) covers Northern European languages.
        let sample = "Pērkons";
        let (encoded, _, had_errors) = ISO_8859_4.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-4 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_5_cyrillic_text() {
        // ISO-8859-5 (code page 28595) is another Cyrillic encoding.
        let sample = "Привет";
        let (encoded, _, had_errors) = ISO_8859_5.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-5 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_6_arabic_text() {
        // ISO-8859-6 (code page 28596) covers Arabic characters.
        let sample = "مرحبا";
        let (encoded, _, had_errors) = ISO_8859_6.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-6 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_7_greek_text() {
        // ISO-8859-7 (code page 28597) covers Greek characters.
        let sample = "Καλημέρα";
        let (encoded, _, had_errors) = ISO_8859_7.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-7 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_8_hebrew_text() {
        // ISO-8859-8 (code page 28598) covers Hebrew characters.
        let sample = "שלום";
        let (encoded, _, had_errors) = ISO_8859_8.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-8 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_10_nordic_text() {
        // ISO-8859-10 (code page 28600) covers Nordic languages.
        let sample = "Ísland";
        let (encoded, _, had_errors) = ISO_8859_10.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-10 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_iso8859_13_baltic_text() {
        // ISO-8859-13 (code page 28603) covers Baltic languages.
        let sample = "Sveiki";
        let (encoded, _, had_errors) = ISO_8859_13.encode(sample);
        assert!(!had_errors, "failed to encode ISO-8859-13 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_koi8_russian_text() {
        // KOI8-R encoding for Russian text.
        let sample = "Привет";
        let (encoded, _, had_errors) = encoding_rs::KOI8_R.encode(sample);
        assert!(!had_errors, "failed to encode KOI8-R sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_koi8_u_ukrainian_text() {
        // KOI8-U encoding for Ukrainian text.
        let sample = "Привіт";
        let (encoded, _, had_errors) = encoding_rs::KOI8_U.encode(sample);
        assert!(!had_errors, "failed to encode KOI8-U sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_874_thai_text() {
        // Windows-874 encoding for Thai text.
        let sample = "สวัสดี";
        let (encoded, _, had_errors) = WINDOWS_874.encode(sample);
        assert!(!had_errors, "failed to encode Windows-874 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1250_central_european_text() {
        // Windows-1250 encoding for Central European languages.
        let sample = "Dobrý den";
        let (encoded, _, had_errors) = WINDOWS_1250.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1250 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1251_cyrillic_text() {
        // Windows-1251 encoding for Cyrillic text.
        let sample = "Привет";
        let (encoded, _, had_errors) = WINDOWS_1251.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1251 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1253_greek_text() {
        // Windows-1253 encoding for Greek text.
        let sample = "Γειά σου";
        let (encoded, _, had_errors) = WINDOWS_1253.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1253 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1254_turkish_text() {
        // Windows-1254 encoding for Turkish text.
        let sample = "İstanbul";
        let (encoded, _, had_errors) = WINDOWS_1254.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1254 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1255_hebrew_text() {
        // Windows-1255 encoding for Hebrew text.
        let sample = "שלום";
        let (encoded, _, had_errors) = WINDOWS_1255.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1255 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1256_arabic_text() {
        // Windows-1256 encoding for Arabic text.
        let sample = "مرحبا";
        let (encoded, _, had_errors) = WINDOWS_1256.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1256 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1257_baltic_text() {
        // Windows-1257 encoding for Baltic languages.
        let sample = "Pērkons";
        let (encoded, _, had_errors) = WINDOWS_1257.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1257 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_windows_1258_vietnamese_text() {
        // Windows-1258 encoding for Vietnamese text.
        let sample = "Xin chào";
        let (encoded, _, had_errors) = WINDOWS_1258.encode(sample);
        assert!(!had_errors, "failed to encode Windows-1258 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_big5_traditional_chinese_text() {
        // Big5 encoding for Traditional Chinese text.
        let sample = "繁體";
        let (encoded, _, had_errors) = BIG5.encode(sample);
        assert!(!had_errors, "failed to encode Big5 sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_shift_jis_japanese_text() {
        // Shift_JIS encoding for Japanese text.
        let sample = "こんにちは";
        let (encoded, _, had_errors) = SHIFT_JIS.encode(sample);
        assert!(!had_errors, "failed to encode Shift_JIS sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_euc_kr_korean_text() {
        // EUC-KR encoding for Korean text.
        let sample = "안녕하세요";
        let (encoded, _, had_errors) = EUC_KR.encode(sample);
        assert!(!had_errors, "failed to encode EUC-KR sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_gbk_simplified_chinese_text() {
        // GBK encoding for Simplified Chinese text.
        let sample = "简体";
        let (encoded, _, had_errors) = GBK.encode(sample);
        assert!(!had_errors, "failed to encode GBK sample");
        assert_eq!(bytes_to_string_smart(encoded.as_ref()), sample);
    }

    #[test]
    fn test_utf8_latin1_mixed_text() {
        // Mixed ASCII and Latin-1 characters should remain intact.
        let bytes = b"caf\xe9";
        assert_eq!(bytes_to_string_smart(bytes), "caf\u{e9}");
    }

    #[test]
    fn test_escape_sequences_preserved() {
        // ANSI escape sequences should remain intact.
        let bytes = b"\x1b[31mred\x1b[0m";
        assert_eq!(bytes_to_string_smart(bytes), "\x1b[31mred\x1b[0m");
    }

    #[test]
    fn test_invalid_utf8_fallback() {
        // Invalid UTF-8 should fall back to lossy decoding.
        let invalid_bytes = vec![0x80, 0x81, 0x82];
        let result = bytes_to_string_smart(&invalid_bytes);
        assert_eq!(result, String::from_utf8_lossy(&invalid_bytes));
    }
}

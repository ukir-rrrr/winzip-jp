//! ファイル名の CP932 化、読み取り時の判定、Mac ジャンク除外。

use std::path::Path;

use encoding_rs::{DecoderResult, EncoderResult, SHIFT_JIS};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// APPNOTE general purpose bit 11（Language encoding flag / EFS）。
pub const EFS_UTF8_BIT: u16 = 1 << 11;

/// Info-ZIP Unicode Path Extra Field（0x7075）。
pub const UNICODE_PATH_EXTRA_ID: u16 = 0x7075;

/// 人名などに多い異体字。IBM 拡張で CP932 に載っていても統合漢字へ寄せる。
///
/// `encode_entry_name` はこの表を `encode_without_replacement` より先に適用する。
pub const COMPAT_IDEOGRAPH_REPLACEMENTS: &[(char, char)] = &[
    ('\u{9AD9}', '\u{9AD8}'), // 髙 → 高
    ('\u{FA11}', '\u{5D0E}'), // 﨑 → 崎
];

/// WHATWG Shift_JIS が ASCII に落とす文字と、Windows CP932 では全角形で持つ記号。
const SYMBOL_REPLACEMENTS: &[(char, char)] = &[
    ('\u{00A5}', '\u{FFE5}'), // ¥ → ￥（単バイト 0x5C 回避）
    ('\u{203E}', '\u{FFE3}'), // ‾ → ￣（単バイト 0x7E 回避）
    ('\u{301C}', '\u{FF5E}'), // 〜 → ～
    ('\u{2212}', '\u{FF0D}'), // − → －
    ('\u{00A2}', '\u{FFE0}'), // ¢ → ￠
    ('\u{00A3}', '\u{FFE1}'), // £ → ￡
    ('\u{00AC}', '\u{FFE2}'), // ¬ → ￢
    ('\u{2016}', '\u{2225}'), // ‖ → ∥
    ('\u{2014}', '\u{2015}'), // — → ―
];

const CONF_EFS_MAJORITY: f32 = 0.95;
const CONF_UNICODE_PATH_MAJORITY: f32 = 0.9;
const CONF_CONTENT: f32 = 0.8;
const CONF_ALL_ASCII: f32 = 0.5;
const CONF_AMBIGUOUS: f32 = 0.4;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameEncoding {
    Cp932,
    Utf8,
    Cp437,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnmappableKind {
    /// 互換漢字・異体字の寄せ。NFKC で CP932 可能になった互換文字（ﬁ など）も含む。
    CompatIdeograph,
    Emoji,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Unmappable {
    /// NFC。入力側の表示名（置換前）。
    pub relative_path: String,
    pub original_char: String,
    pub suggested: String,
    pub kind: UnmappableKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncodedName {
    /// `/` 区切り。末尾 `/` はディレクトリ。
    pub cp932: Vec<u8>,
    pub issues: Vec<Unmappable>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub encoding: NameEncoding,
    /// 0.0–1.0。0.6 未満なら UI で切替を強調する。
    pub confidence: f32,
    pub ambiguous: bool,
}

/// Unicode の ZIP エントリ名を NFC → 置換表 → CP932 にする。
///
/// パス成分が `.` または `..` のときは、置換の有無にかかわらず `_` にする
/// （ZIP 名にパストラバーサル成分を残さない）。
/// 同一 ZIP 内のバイト列衝突（` (2)` 連番）は `plan_write`（PR 2）側の責務。
pub fn encode_entry_name(unicode_path: &str) -> EncodedName {
    let slash_path = unicode_path.replace('\\', "/");
    let trailing_slash = slash_path.ends_with('/');
    let mut nfc_components = Vec::new();
    for component in slash_path.split('/') {
        if component.is_empty() {
            continue;
        }
        nfc_components.push(component.nfc().collect::<String>());
    }
    let relative_path = join_slash(&nfc_components, trailing_slash);

    let mut issues = Vec::new();
    let mut encoded_components = Vec::with_capacity(nfc_components.len());
    for component in &nfc_components {
        encoded_components.push(encode_component(component, &relative_path, &mut issues));
    }

    let mut cp932 = join_cp932(&encoded_components);
    if trailing_slash && !cp932.ends_with(b"/") {
        cp932.push(b'/');
    }

    EncodedName { cp932, issues }
}

/// アーカイブ単位で読み方を 1 つ決める。混在は手動切替に倒す。
///
/// 判定順は設計 §3.4：bit 11 過半数 → 0x7075 CRC 一致過半数 → 全 ASCII → 非 ASCII 件数。
pub fn detect_encoding(entries: &[(u16, Vec<u8>, Vec<u8>)]) -> Detection {
    let total = entries.len();

    let efs_count = entries
        .iter()
        .filter(|(gpbf, _, _)| gpbf & EFS_UTF8_BIT != 0)
        .count();
    if is_majority(efs_count, total) {
        return Detection {
            encoding: NameEncoding::Utf8,
            confidence: CONF_EFS_MAJORITY,
            ambiguous: false,
        };
    }

    let unicode_path_count = entries
        .iter()
        .filter(|(_, raw, extra)| extra_has_matching_unicode_path(raw, extra))
        .count();
    if is_majority(unicode_path_count, total) {
        return Detection {
            encoding: NameEncoding::Utf8,
            confidence: CONF_UNICODE_PATH_MAJORITY,
            ambiguous: false,
        };
    }

    if total == 0 || entries.iter().all(|(_, raw, _)| raw.is_ascii()) {
        return Detection {
            encoding: NameEncoding::Cp932,
            confidence: CONF_ALL_ASCII,
            ambiguous: false,
        };
    }

    let mut utf8_ok = 0usize;
    let mut cp932_ok = 0usize;
    for (_, raw, _) in entries {
        if raw.is_ascii() {
            continue;
        }
        if std::str::from_utf8(raw).is_ok() {
            utf8_ok += 1;
        }
        if is_valid_cp932(raw) {
            cp932_ok += 1;
        }
    }

    if cp932_ok > utf8_ok {
        return Detection {
            encoding: NameEncoding::Cp932,
            confidence: CONF_CONTENT,
            ambiguous: false,
        };
    }
    if utf8_ok > cp932_ok {
        return Detection {
            encoding: NameEncoding::Utf8,
            confidence: CONF_CONTENT,
            ambiguous: false,
        };
    }

    Detection {
        encoding: NameEncoding::Cp932,
        confidence: CONF_AMBIGUOUS,
        ambiguous: true,
    }
}

pub fn decode_name(raw: &[u8], encoding: NameEncoding) -> String {
    match encoding {
        NameEncoding::Utf8 => String::from_utf8_lossy(raw).into_owned(),
        NameEncoding::Cp932 => {
            let (decoded, _, _) = SHIFT_JIS.decode(raw);
            decoded.into_owned()
        }
        NameEncoding::Cp437 => raw.iter().map(|&b| CP437[b as usize]).collect(),
    }
}

/// パス成分単位。`.DS_Store` と `__MACOSX` のみ（`._*` は v1.1）。
pub fn is_junk(rel: impl AsRef<Path>) -> bool {
    rel.as_ref()
        .iter()
        .any(|c| c == ".DS_Store" || c == "__MACOSX")
}

fn encode_component(nfc: &str, relative_path: &str, issues: &mut Vec<Unmappable>) -> Vec<u8> {
    let mut mapped = String::new();
    for grapheme in nfc.graphemes(true) {
        match replace_grapheme(grapheme) {
            GraphemeMap::Keep { text } => mapped.push_str(&text),
            GraphemeMap::Replace { suggested, kind } => {
                issues.push(Unmappable {
                    relative_path: relative_path.to_string(),
                    original_char: grapheme.to_string(),
                    suggested: suggested.clone(),
                    kind,
                });
                mapped.push_str(&suggested);
            }
        }
    }

    // `.` / `..` は半角のまま残さない（設計 §3.2。置換を経由しなくても同じ）。
    if mapped.is_empty() || mapped == "." || mapped == ".." {
        if mapped == "." || mapped == ".." {
            issues.push(Unmappable {
                relative_path: relative_path.to_string(),
                original_char: mapped.clone(),
                suggested: "_".to_string(),
                kind: UnmappableKind::Other,
            });
        }
        mapped = "_".to_string();
    }

    encode_cp932_without_replacement(&mapped).unwrap_or_else(|_| b"_".to_vec())
}

enum GraphemeMap {
    Keep {
        text: String,
    },
    Replace {
        suggested: String,
        kind: UnmappableKind,
    },
}

fn replace_grapheme(grapheme: &str) -> GraphemeMap {
    let stripped: String = grapheme
        .chars()
        .filter(|c| !is_variation_selector(*c))
        .collect();
    let g = if stripped.is_empty() {
        grapheme
    } else {
        stripped.as_str()
    };

    let mut chars = g.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if let Some((to, kind)) = forced_replacement(c) {
            return GraphemeMap::Replace {
                suggested: to.to_string(),
                kind,
            };
        }
    }

    if encode_cp932_without_replacement(g).is_ok() {
        return GraphemeMap::Keep {
            text: g.to_string(),
        };
    }

    let nfkc: String = g.nfkc().collect();
    if nfkc != g && encode_cp932_without_replacement(&nfkc).is_ok() && nfkc_is_safe_component(&nfkc)
    {
        return GraphemeMap::Replace {
            suggested: nfkc,
            kind: UnmappableKind::CompatIdeograph,
        };
    }

    if g.chars().any(is_emoji_char) || grapheme.chars().any(is_emoji_char) {
        return GraphemeMap::Replace {
            suggested: "_".to_string(),
            kind: UnmappableKind::Emoji,
        };
    }

    GraphemeMap::Replace {
        suggested: "_".to_string(),
        kind: UnmappableKind::Other,
    }
}

fn forced_replacement(c: char) -> Option<(char, UnmappableKind)> {
    for &(from, to) in COMPAT_IDEOGRAPH_REPLACEMENTS {
        if from == c {
            return Some((to, UnmappableKind::CompatIdeograph));
        }
    }
    for &(from, to) in SYMBOL_REPLACEMENTS {
        if from == c {
            return Some((to, UnmappableKind::Other));
        }
    }
    None
}

fn nfkc_is_safe_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.contains('\\')
}

fn is_variation_selector(c: char) -> bool {
    matches!(c as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF)
}

fn encode_cp932_without_replacement(s: &str) -> Result<Vec<u8>, char> {
    let mut encoder = SHIFT_JIS.new_encoder();
    let mut out = Vec::with_capacity(s.len() * 2);
    let mut rest = s;
    loop {
        if rest.is_empty() {
            return Ok(out);
        }
        let mut buf = [0u8; 128];
        let (result, read, written) =
            encoder.encode_from_utf8_without_replacement(rest, &mut buf, true);
        out.extend_from_slice(&buf[..written]);
        rest = &rest[read..];
        match result {
            EncoderResult::InputEmpty => return Ok(out),
            EncoderResult::OutputFull => continue,
            EncoderResult::Unmappable(c) => return Err(c),
        }
    }
}

fn is_valid_cp932(raw: &[u8]) -> bool {
    let mut decoder = SHIFT_JIS.new_decoder_without_bom_handling();
    let mut decoded = String::with_capacity(raw.len());
    let mut remaining = raw;
    loop {
        let needed = decoder.max_utf8_buffer_length_without_replacement(remaining.len());
        decoded.reserve(needed.unwrap_or(remaining.len() * 3 + 8));
        let (result, read) =
            decoder.decode_to_string_without_replacement(remaining, &mut decoded, true);
        remaining = &remaining[read..];
        match result {
            DecoderResult::InputEmpty => return remaining.is_empty(),
            DecoderResult::Malformed(_, _) => return false,
            DecoderResult::OutputFull => continue,
        }
    }
}

fn extra_has_matching_unicode_path(raw_name: &[u8], extra: &[u8]) -> bool {
    let expected_crc = crc32fast::hash(raw_name);
    let mut i = 0;
    while i + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[i], extra[i + 1]]);
        let size = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
        i += 4;
        if i + size > extra.len() {
            return false;
        }
        let data = &extra[i..i + size];
        i += size;
        if id != UNICODE_PATH_EXTRA_ID || data.len() < 5 {
            continue;
        }
        let version = data[0];
        let crc = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        if version == 1 && crc == expected_crc {
            return true;
        }
    }
    false
}

fn is_majority(count: usize, total: usize) -> bool {
    total > 0 && count * 2 > total
}

fn join_slash(components: &[String], trailing_slash: bool) -> String {
    let mut s = components.join("/");
    if trailing_slash && !s.ends_with('/') {
        s.push('/');
    }
    s
}

fn join_cp932(components: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, part) in components.iter().enumerate() {
        if i > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    out
}

fn is_emoji_char(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x00A9
            | 0x00AE
            | 0x200D
            | 0x203C
            | 0x2049
            | 0x20E3
            | 0x2122
            | 0x2139
            | 0x2328
            | 0x23CF
            | 0x24C2
            | 0x25B6
            | 0x25C0
            | 0x2934
            | 0x2935
            | 0x2B50
            | 0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0xFE0F
    ) || (0x2194..=0x2199).contains(&u)
        || (0x21A9..=0x21AA).contains(&u)
        || (0x231A..=0x231B).contains(&u)
        || (0x23E9..=0x23F3).contains(&u)
        || (0x23F8..=0x23FA).contains(&u)
        || (0x25AA..=0x25AB).contains(&u)
        || (0x25FB..=0x25FE).contains(&u)
        || (0x2600..=0x27BF).contains(&u)
        || (0x2B05..=0x2B07).contains(&u)
        || (0x2B1B..=0x2B1C).contains(&u)
        || (0x1F000..=0x1FFFF).contains(&u)
        || (0xE0020..=0xE007F).contains(&u)
}

/// IBM CP437。0x00–0x7F は ZIP 名向けにバイト値そのもの。
const CP437: [char; 256] = {
    let mut t = ['\0'; 256];
    let mut i = 0;
    while i < 128 {
        t[i] = i as u8 as char;
        i += 1;
    }
    let high: [char; 128] = [
        '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}',
        '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}',
        '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}',
        '\u{00F2}', '\u{00FB}', '\u{00F9}', '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}',
        '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}',
        '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{2310}',
        '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}', '\u{2591}',
        '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
        '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}',
        '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}',
        '\u{255E}', '\u{255F}', '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}',
        '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
        '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}',
        '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}', '\u{03B1}', '\u{00DF}',
        '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}',
        '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
        '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}',
        '\u{2248}', '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}',
        '\u{25A0}', '\u{00A0}',
    ];
    i = 0;
    while i < 128 {
        t[128 + i] = high[i];
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use unicode_normalization::UnicodeNormalization;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    fn cp932(s: &str) -> Vec<u8> {
        encode_cp932_without_replacement(s).expect("fixture must be CP932-encodable")
    }

    fn extra_7075(raw: &[u8], unicode: &str) -> Vec<u8> {
        let unicode_bytes = unicode.as_bytes();
        let crc = crc32fast::hash(raw);
        let data_len = 1 + 4 + unicode_bytes.len();
        let mut extra = Vec::with_capacity(4 + data_len);
        extra.extend_from_slice(&UNICODE_PATH_EXTRA_ID.to_le_bytes());
        extra.extend_from_slice(&(data_len as u16).to_le_bytes());
        extra.push(1);
        extra.extend_from_slice(&crc.to_le_bytes());
        extra.extend_from_slice(unicode_bytes);
        extra
    }

    fn assert_conf(got: f32, want: f32) {
        assert!((got - want).abs() < 1e-6, "confidence {got} != {want}");
    }

    #[test]
    fn nfd_ga_encodes_to_nfc_cp932_hex() {
        let nfc = "が";
        let nfd: String = nfc.nfd().collect();
        assert_ne!(nfd, nfc, "fixture must be decomposed");
        assert_eq!(nfd.chars().count(), 2);

        let encoded = encode_entry_name(&nfd);
        assert_eq!(hex(&encoded.cp932), "82AA");
        assert_eq!(encoded.cp932, cp932(nfc));
        assert!(encoded.issues.is_empty());
        assert_eq!(decode_name(&encoded.cp932, NameEncoding::Cp932), nfc);
    }

    #[test]
    fn nfd_ga_in_path_is_nfc_per_component() {
        let nfd: String = "書類/が.txt".nfd().collect();
        let encoded = encode_entry_name(&nfd);
        assert_eq!(encoded.cp932, cp932("書類/が.txt"));
        assert!(encoded.issues.is_empty());
    }

    #[test]
    fn taka_replaced_with_unified_even_if_ibm_ext_encodes() {
        // encoding_rs の SHIFT_JIS は IBM 拡張で 髙 を FBFC にできるが、設計どおり 高 へ寄せる。
        assert!(encode_cp932_without_replacement("髙").is_ok());
        let encoded = encode_entry_name("髙.txt");
        assert_eq!(hex(&encoded.cp932), "8D822E747874");
        assert_eq!(encoded.cp932, [0x8D, 0x82, 0x2E, 0x74, 0x78, 0x74]);
        assert_eq!(encoded.issues.len(), 1);
        assert_eq!(encoded.issues[0].original_char, "髙");
        assert_eq!(encoded.issues[0].suggested, "高");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::CompatIdeograph);
        assert_eq!(encoded.issues[0].relative_path, "髙.txt");
        assert!(COMPAT_IDEOGRAPH_REPLACEMENTS.contains(&('\u{9AD9}', '\u{9AD8}')));
    }

    #[test]
    fn saki_compat_replaced_with_unified() {
        let encoded = encode_entry_name("﨑");
        assert_eq!(hex(&encoded.cp932), "8DE8");
        assert_eq!(encoded.issues[0].suggested, "崎");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::CompatIdeograph);
    }

    #[test]
    fn emoji_becomes_underscore() {
        let encoded = encode_entry_name("写真🎉.jpg");
        assert_eq!(hex(&encoded.cp932), "8ECA905E5F2E6A7067");
        assert_eq!(encoded.issues.len(), 1);
        assert_eq!(encoded.issues[0].original_char, "🎉");
        assert_eq!(encoded.issues[0].suggested, "_");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::Emoji);
    }

    #[test]
    fn flag_grapheme_becomes_single_underscore() {
        let encoded = encode_entry_name("🇯🇵.txt");
        assert_eq!(encoded.cp932, b"_.txt");
        assert_eq!(hex(&encoded.cp932), "5F2E747874");
        assert_eq!(encoded.issues.len(), 1);
        assert_eq!(encoded.issues[0].kind, UnmappableKind::Emoji);
    }

    #[test]
    fn yen_and_overline_do_not_become_ascii_backslash_or_tilde() {
        let yen = encode_entry_name("¥100.txt");
        assert!(!yen.cp932.contains(&0x5C), "U+00A5 must not encode as 0x5C");
        assert_eq!(hex(&yen.cp932), "818F3130302E747874");
        assert_eq!(yen.issues.len(), 1);
        assert_eq!(yen.issues[0].original_char, "¥");
        assert_eq!(yen.issues[0].suggested, "￥");
        assert_eq!(yen.issues[0].kind, UnmappableKind::Other);

        let overline = encode_entry_name("\u{203E}");
        assert_ne!(overline.cp932, b"~");
        assert_eq!(hex(&overline.cp932), "8150");
        assert_eq!(overline.issues[0].suggested, "￣");
    }

    #[test]
    fn wave_dash_maps_to_fullwidth_tilde() {
        let encoded = encode_entry_name("〜");
        assert_eq!(hex(&encoded.cp932), "8160");
        assert_eq!(encoded.issues[0].original_char, "〜");
        assert_eq!(encoded.issues[0].suggested, "～");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::Other);
    }

    #[test]
    fn nfkc_must_not_inject_slash_or_backslash() {
        let encoded = encode_entry_name("℅.txt");
        assert_eq!(encoded.cp932, b"_.txt");
        assert!(!encoded.cp932.contains(&b'/'));
        assert!(!encoded.cp932.contains(&b'\\'));
        assert_eq!(encoded.issues[0].suggested, "_");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::Other);
    }

    #[test]
    fn ideographic_variation_selector_is_stripped() {
        let encoded = encode_entry_name("辻\u{E0100}.txt");
        assert_eq!(hex(&encoded.cp932), "92D22E747874");
        assert!(encoded.issues.is_empty());
    }

    #[test]
    fn nec_ibm_symbols_pin_cp932_hex() {
        assert_eq!(hex(&encode_entry_name("①").cp932), "8740");
        assert_eq!(hex(&encode_entry_name("ⅱ").cp932), "FA41");
        assert_eq!(hex(&encode_entry_name("㈱").cp932), "878A");
        assert_eq!(hex(&encode_entry_name("～").cp932), "8160");
        assert_eq!(hex(&encode_entry_name("￥").cp932), "818F");
        assert_eq!(hex(&encode_entry_name("－").cp932), "817C");
        assert_eq!(hex(&encode_entry_name("¢").cp932), "8191");
        assert_eq!(hex(&encode_entry_name("£").cp932), "8192");
        assert_eq!(hex(&encode_entry_name("¬").cp932), "81CA");
    }

    #[test]
    fn windows_lookalike_symbols_are_replaced_not_underscore() {
        let minus = encode_entry_name("−");
        assert_eq!(hex(&minus.cp932), "817C");
        assert_eq!(minus.issues[0].suggested, "－");
        assert_eq!(minus.issues[0].kind, UnmappableKind::Other);

        let dbl_bar = encode_entry_name("‖");
        assert_eq!(hex(&dbl_bar.cp932), "8161");
        assert_eq!(dbl_bar.issues[0].suggested, "∥");

        let em_dash = encode_entry_name("—");
        assert_eq!(hex(&em_dash.cp932), "815C");
        assert_eq!(em_dash.issues[0].suggested, "―");
        assert_ne!(em_dash.cp932, b"_");
    }

    #[test]
    fn dot_and_dotdot_components_always_become_underscore() {
        let dot = encode_entry_name(".");
        assert_eq!(dot.cp932, b"_");
        assert_eq!(dot.issues[0].original_char, ".");
        assert_eq!(dot.issues[0].suggested, "_");

        let dotdot = encode_entry_name("..");
        assert_eq!(dotdot.cp932, b"_");
        assert_eq!(dotdot.issues[0].original_char, "..");

        let nested = encode_entry_name("foo/../bar");
        assert_eq!(nested.cp932, b"foo/_/bar");
        assert!(nested.issues.iter().any(|i| i.original_char == ".."));

        let hidden = encode_entry_name(".DS_Store");
        assert_eq!(hidden.cp932, b".DS_Store");
    }

    #[test]
    fn cjk_compat_with_canonical_decomp_is_nfc_before_issues() {
        // U+F90A 金 は canonical 分解を持つので、置換表より先の NFC で 金 になる。
        let encoded = encode_entry_name("金");
        assert_eq!(encoded.cp932, cp932("金"));
        assert!(encoded.issues.is_empty());
    }

    #[test]
    fn compat_letter_surviving_nfc_maps_via_nfkc() {
        // ﬁ は NFC では残るが CP932 不能。NFKC で fi。
        assert_eq!("ﬁ".nfc().collect::<String>(), "ﬁ");
        assert!(encode_cp932_without_replacement("ﬁ").is_err());
        let encoded = encode_entry_name("ﬁ.txt");
        assert_eq!(encoded.cp932, b"fi.txt");
        assert_eq!(encoded.issues.len(), 1);
        assert_eq!(encoded.issues[0].suggested, "fi");
        assert_eq!(encoded.issues[0].kind, UnmappableKind::CompatIdeograph);
    }

    #[test]
    fn backslash_becomes_slash_and_trailing_slash_kept() {
        let encoded = encode_entry_name("dir\\sub\\");
        assert_eq!(encoded.cp932, b"dir/sub/");
    }

    #[test]
    fn ascii_name_is_still_cp932_bytes() {
        let encoded = encode_entry_name("hello.txt");
        assert_eq!(encoded.cp932, b"hello.txt");
        assert!(encoded.issues.is_empty());
    }

    #[test]
    fn detect_encoding_table() {
        let ga_cp932 = cp932("が.txt");
        let ga_utf8 = "が.txt".as_bytes().to_vec();
        let ascii = b"readme.txt".to_vec();
        let docs_utf8 = "ドキュメント.txt".as_bytes().to_vec();
        let docs_cp932 = cp932("ドキュメント.txt");

        struct Case {
            name: &'static str,
            entries: Vec<(u16, Vec<u8>, Vec<u8>)>,
            encoding: NameEncoding,
            confidence: f32,
            ambiguous: bool,
        }

        let cases = [
            Case {
                name: "bit11 majority → UTF-8 0.95",
                entries: vec![
                    (EFS_UTF8_BIT, ga_utf8.clone(), vec![]),
                    (EFS_UTF8_BIT, docs_utf8.clone(), vec![]),
                    (0, ascii.clone(), vec![]),
                ],
                encoding: NameEncoding::Utf8,
                confidence: 0.95,
                ambiguous: false,
            },
            Case {
                name: "bit11 absent / Finder-style UTF-8",
                entries: vec![(0, ga_utf8.clone(), vec![]), (0, docs_utf8.clone(), vec![])],
                encoding: NameEncoding::Utf8,
                confidence: 0.8,
                ambiguous: false,
            },
            Case {
                name: "CP932 typical, bit11 down",
                entries: vec![
                    (0, ga_cp932.clone(), vec![]),
                    (0, docs_cp932.clone(), vec![]),
                    (0, ascii.clone(), vec![]),
                ],
                encoding: NameEncoding::Cp932,
                confidence: 0.8,
                ambiguous: false,
            },
            Case {
                name: "all ASCII → CP932 0.5",
                entries: vec![(0, ascii.clone(), vec![]), (0, b"a/b".to_vec(), vec![])],
                encoding: NameEncoding::Cp932,
                confidence: 0.5,
                ambiguous: false,
            },
            Case {
                name: "bit11 majority on ASCII still UTF-8 0.95 (design order)",
                entries: vec![
                    (EFS_UTF8_BIT, ascii.clone(), vec![]),
                    (EFS_UTF8_BIT, b"a/b".to_vec(), vec![]),
                ],
                encoding: NameEncoding::Utf8,
                confidence: 0.95,
                ambiguous: false,
            },
            Case {
                name: "0x7075 CRC match majority → UTF-8 0.9",
                entries: vec![
                    (0, ascii.clone(), extra_7075(&ascii, "readme.txt")),
                    (0, ga_cp932.clone(), extra_7075(&ga_cp932, "が.txt")),
                    (0, b"x".to_vec(), vec![]),
                ],
                encoding: NameEncoding::Utf8,
                confidence: 0.9,
                ambiguous: false,
            },
            Case {
                name: "0x7075 CRC mismatch does not count",
                entries: vec![(0, ga_cp932.clone(), {
                    let mut extra = extra_7075(&ga_cp932, "が.txt");
                    extra[8] ^= 0xFF;
                    extra
                })],
                encoding: NameEncoding::Cp932,
                confidence: 0.8,
                ambiguous: false,
            },
            Case {
                name: "tie non-ASCII → CP932 0.4 ambiguous",
                entries: vec![(0, ga_utf8.clone(), vec![]), (0, ga_cp932.clone(), vec![])],
                encoding: NameEncoding::Cp932,
                confidence: 0.4,
                ambiguous: true,
            },
            Case {
                name: "empty archive treated like all ASCII",
                entries: vec![],
                encoding: NameEncoding::Cp932,
                confidence: 0.5,
                ambiguous: false,
            },
        ];

        for case in cases {
            let got = detect_encoding(&case.entries);
            assert_eq!(got.encoding, case.encoding, "{}", case.name);
            assert_conf(got.confidence, case.confidence);
            assert_eq!(got.ambiguous, case.ambiguous, "{}", case.name);
        }
    }

    #[test]
    fn bit11_not_majority_falls_through_to_bytes() {
        let entries = vec![
            (EFS_UTF8_BIT, "が.txt".as_bytes().to_vec(), vec![]),
            (0, cp932("が.txt"), vec![]),
            (0, b"a.txt".to_vec(), vec![]),
        ];
        let got = detect_encoding(&entries);
        assert_eq!(got.encoding, NameEncoding::Cp932);
        assert!(got.ambiguous);
        assert_conf(got.confidence, 0.4);
    }

    #[test]
    fn decode_name_roundtrips_fixtures() {
        let raw = cp932("が");
        assert_eq!(decode_name(&raw, NameEncoding::Cp932), "が");
        assert_eq!(decode_name("が".as_bytes(), NameEncoding::Utf8), "が");
        assert_eq!(decode_name(&[0x82], NameEncoding::Cp437), "é");
    }

    #[test]
    fn junk_detection_table() {
        let cases: &[(&str, bool)] = &[
            (".DS_Store", true),
            ("__MACOSX", true),
            ("foo/.DS_Store", true),
            ("foo/bar/.DS_Store", true),
            ("__MACOSX/._file", true),
            ("foo/__MACOSX/bar.txt", true),
            ("dir/__MACOSX", true),
            ("file.txt", false),
            ("foo/bar.txt", false),
            ("._hidden", false),
            ("foo/._bar", false),
            (".DS_Store.bak", false),
            ("not__MACOSX", false),
            ("__macosx", false),
            (".ds_store", false),
            ("DS_Store", false),
        ];
        for &(path, junk) in cases {
            assert_eq!(is_junk(Path::new(path)), junk, "{path}");
            assert_eq!(is_junk(PathBuf::from(path)), junk, "{path} PathBuf");
        }
    }
}

//! ZIP エントリ名の脱出判定と、ディレクトリ印の文字境界。

use std::path::{Component, Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameReject {
    /// `..` / 絶対パス / ドライブレター / `:` / 区切りとしての `\`。
    Escape,
    /// NUL・空名・予約名・末尾空白ドットなど。アーカイブごと失敗するが脱出ではない。
    Invalid(&'static str),
}

pub(crate) fn classify_name(raw: &[u8], display: &str) -> Option<NameReject> {
    if name_escapes(raw, display) {
        return Some(NameReject::Escape);
    }
    if raw.contains(&0) || display.contains('\0') {
        return Some(NameReject::Invalid("NUL文字"));
    }
    let unified = display.replace('\\', "/");
    let trimmed = unified.trim_matches('/');
    if trimmed.is_empty() || trimmed.split('/').all(|c| c.is_empty() || c == ".") {
        return Some(NameReject::Invalid("空の名前"));
    }
    if has_invalid_windows_name(raw, display) {
        return Some(NameReject::Invalid("この名前は使えません"));
    }
    None
}

pub(crate) fn unsafe_component(component: &str) -> bool {
    component_reject(component).is_some()
}

/// 末尾 `/`、または先頭から見たとき単独バイトの末尾 `\` だけをディレクトリとみなす。
pub(crate) fn zip_name_is_dir(raw: &[u8]) -> bool {
    if raw.ends_with(b"/") {
        return true;
    }
    if !raw.ends_with(b"\\") {
        return false;
    }
    let mut i = 0;
    let mut last_start = 0;
    while i < raw.len() {
        last_start = i;
        i += next_raw_unit(raw, i);
    }
    last_start + 1 == raw.len() && raw[last_start] == b'\\'
}

fn name_escapes(raw: &[u8], display: &str) -> bool {
    raw_escape(raw) || display_escape(display)
}

fn has_invalid_windows_name(raw: &[u8], display: &str) -> bool {
    for_each_raw_component(raw, |part| {
        is_reserved_dos_bytes(part) || trailing_bad_bytes(part)
    }) || display.replace('\\', "/").split('/').any(|c| {
        !c.is_empty() && c != "." && matches!(component_reject(c), Some(NameReject::Invalid(_)))
    })
}

fn component_reject(component: &str) -> Option<NameReject> {
    if component == "." {
        return None;
    }
    if component.is_empty() {
        return Some(NameReject::Invalid("空の名前"));
    }
    if component == ".." {
        return Some(NameReject::Escape);
    }
    if component.contains('\0') {
        return Some(NameReject::Invalid("NUL文字"));
    }
    if component.contains('\\') || component.contains('/') || looks_like_drive(component) {
        return Some(NameReject::Escape);
    }
    if component.contains(':') {
        return Some(NameReject::Invalid("この名前は使えません"));
    }
    if is_reserved_dos_name(component) {
        return Some(NameReject::Invalid("この名前は使えません"));
    }
    let trimmed = component.trim_end_matches([' ', '.']);
    if trimmed.is_empty() || trimmed != component {
        return Some(NameReject::Invalid("この名前は使えません"));
    }
    None
}

fn is_cp932_lead(b: u8) -> bool {
    (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b)
}

fn is_cp932_trail(b: u8) -> bool {
    (0x40..=0x7E).contains(&b) || (0x80..=0xFC).contains(&b)
}

fn next_raw_unit(raw: &[u8], i: usize) -> usize {
    if i + 1 < raw.len() && is_cp932_lead(raw[i]) && is_cp932_trail(raw[i + 1]) {
        2
    } else {
        1
    }
}

fn for_each_raw_component(raw: &[u8], mut visit: impl FnMut(&[u8]) -> bool) -> bool {
    let mut i = 0;
    let mut start = 0;
    while i < raw.len() {
        let step = next_raw_unit(raw, i);
        if step == 1 && (raw[i] == b'/' || raw[i] == b'\\') {
            if visit(&raw[start..i]) {
                return true;
            }
            start = i + 1;
        }
        i += step;
    }
    visit(&raw[start..])
}

fn raw_starts_with_drive(raw: &[u8]) -> bool {
    raw.len() >= 2 && raw[0].is_ascii_alphabetic() && raw[1] == b':' && next_raw_unit(raw, 0) == 1
}

fn raw_escape(raw: &[u8]) -> bool {
    if raw.is_empty() {
        return false;
    }
    if raw[0] == b'/' || raw[0] == b'\\' {
        return true;
    }
    if raw_starts_with_drive(raw) {
        return true;
    }
    for_each_raw_component(raw, |part| part == b"..")
}

fn display_escape(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let unified = name.replace('\\', "/");
    if unified.starts_with('/') || looks_like_drive(&unified) {
        return true;
    }
    for component in Path::new(&unified).components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return true,
            Component::Normal(s) => {
                if matches!(
                    component_reject(&s.to_string_lossy()),
                    Some(NameReject::Escape)
                ) {
                    return true;
                }
            }
            Component::CurDir => {}
        }
    }
    unified
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .any(|c| matches!(component_reject(c), Some(NameReject::Escape)))
}

fn looks_like_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

fn is_reserved_dos_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_reserved_dos_bytes(part: &[u8]) -> bool {
    let stem = part.split(|b| *b == b'.').next().unwrap_or(part);
    is_reserved_dos_name(&String::from_utf8_lossy(stem))
}

fn trailing_bad_bytes(part: &[u8]) -> bool {
    if part.is_empty() {
        return false;
    }
    let last = part[part.len() - 1];
    last == b' ' || last == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_are_escape_and_reserved_are_invalid() {
        assert_eq!(
            classify_name(b"../evil", "../evil"),
            Some(NameReject::Escape)
        );
        assert_eq!(
            classify_name(b"/tmp/evil", "/tmp/evil"),
            Some(NameReject::Escape)
        );
        assert_eq!(
            classify_name(br"C:\evil", r"C:\evil"),
            Some(NameReject::Escape)
        );
        assert_eq!(
            classify_name(b"foo:bar", "foo:bar"),
            Some(NameReject::Invalid("この名前は使えません"))
        );
        assert_eq!(
            classify_name(b"Note: 2024.txt", "Note: 2024.txt"),
            Some(NameReject::Invalid("この名前は使えません"))
        );
        assert_eq!(classify_name(b"C:evil", "C:evil"), Some(NameReject::Escape));
        assert_eq!(classify_name(b"ok/file.txt", "ok/file.txt"), None);
        assert_eq!(classify_name(b"dir/", "dir/"), None);
        assert_eq!(classify_name("が.txt".as_bytes(), "が.txt"), None);
        let hyo = [0x95, 0x5C, b'.', b't', b'x', b't'];
        assert_eq!(classify_name(&hyo, "表.txt"), None);
        let so = [0x83, 0x5C, b'.', b't', b'x', b't'];
        assert_eq!(classify_name(&so, "ソ.txt"), None);
        let hyo_nul = [0x95, 0x5C, b'N', b'U', b'L', b'.', b't', b'x', b't'];
        assert_eq!(classify_name(&hyo_nul, "表NUL.txt"), None);
        assert_eq!(
            classify_name(b"", ""),
            Some(NameReject::Invalid("空の名前"))
        );
        assert_eq!(
            classify_name(b"NUL", "NUL"),
            Some(NameReject::Invalid("この名前は使えません"))
        );
        assert_eq!(
            classify_name(b"CON.txt", "CON.txt"),
            Some(NameReject::Invalid("この名前は使えません"))
        );
        assert_eq!(
            classify_name(b"a\0b", "a\0b"),
            Some(NameReject::Invalid("NUL文字"))
        );
    }

    #[test]
    fn zip_name_dir_uses_cp932_boundaries() {
        assert!(!zip_name_is_dir(&[0x95, 0x5C]));
        assert!(!zip_name_is_dir(&[0x83, 0x5C]));
        assert!(!zip_name_is_dir(&[0x88, 0x9F]));
        assert!(zip_name_is_dir(b"dir/"));
        assert!(zip_name_is_dir(br"dir\"));
        assert!(zip_name_is_dir(&[0x95, 0x5C, b'\\']));
        assert!(zip_name_is_dir(&[0x88, 0x9F, b'\\']));
    }
}

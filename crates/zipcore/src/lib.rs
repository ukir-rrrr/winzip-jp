//! Windows 向け ZIP のファイル名とフラグを自前で扱うコア。
//!
//! モジュール境界は `encoding` / `write` / `read` / `split`。実装は PR 1 以降。

/// ワークスペースからこの crate がリンクできることを示すプレースホルダ。
pub fn crate_is_linked() -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn lib_compiles_and_links() {
        assert!(crate::crate_is_linked());
    }
}

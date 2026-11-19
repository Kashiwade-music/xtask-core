//! Role: 文字列の一覧の共通の検証（空・前後の空白・重複）を一か所にまとめる。
use std::collections::BTreeSet;

/// Reject an empty entry, an entry with surrounding whitespace, or a repeated entry.
pub(crate) fn check_names<'a>(
    context: &str,
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in names {
        if name.is_empty() {
            return Err(format!("{context} contains an empty string"));
        }
        if name.trim() != name {
            return Err(format!(
                "{context} contains `{name}` with surrounding whitespace"
            ));
        }
        if !seen.insert(name) {
            return Err(format!("{context} contains duplicate `{name}`"));
        }
    }
    Ok(())
}

/// Check a list with no order and keep it as a set.
pub(crate) fn name_set(context: &str, names: Vec<String>) -> Result<BTreeSet<String>, String> {
    check_names(context, names.iter().map(String::as_str))?;
    Ok(names.into_iter().collect())
}

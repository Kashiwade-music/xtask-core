//! Role: 組み込みderiveと対応macroを、名前の由来に基づいて識別する。
//! Recognize origins, not just spellings, of supported intrinsic grammars.
/// The standard derives, each with the module that provides it.
pub(crate) const DERIVES: &[(&str, &str)] = &[
    ("Clone", "clone"),
    ("Copy", "marker"),
    ("Debug", "fmt"),
    ("Default", "default"),
    ("Hash", "hash"),
    ("Eq", "cmp"),
    ("PartialEq", "cmp"),
    ("Ord", "cmp"),
    ("PartialOrd", "cmp"),
];

/// The attribute macros that the standard prelude provides. An unknown external glob may hide them.
pub(crate) const PRELUDE_ATTRIBUTES: &[&str] = &["test", "bench", "global_allocator", "test_case"];

pub(crate) fn derive(path: &[String]) -> bool {
    let [provider, module, name] = path else {
        return false;
    };
    matches!(provider.as_str(), "std" | "core")
        && DERIVES
            .iter()
            .any(|&(derive, provides)| derive == name && provides == module)
}

pub(crate) fn macro_name(path: &[String]) -> Option<&str> {
    let [provider, name] = path else {
        return None;
    };
    matches!(provider.as_str(), "std" | "core" | "alloc").then_some(name.as_str())
}

//! Role: 組み込みderiveと対応macroを、名前の由来に基づいて識別する。
//! Recognize origins, not just spellings, of supported intrinsic grammars.
pub(crate) const DERIVES: &[&str] = &[
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "Eq",
    "PartialEq",
    "Ord",
    "PartialOrd",
    "Hash",
];

pub(crate) fn derive(path: &[String]) -> bool {
    let [provider, module, name] = path else {
        return false;
    };
    if !matches!(provider.as_str(), "std" | "core") {
        return false;
    }
    match name.as_str() {
        "Clone" => module == "clone",
        "Copy" => module == "marker",
        "Debug" => module == "fmt",
        "Default" => module == "default",
        "Hash" => module == "hash",
        "Eq" | "PartialEq" | "Ord" | "PartialOrd" => module == "cmp",
        _ => false,
    }
}

pub(crate) fn macro_name(path: &[String]) -> Option<&str> {
    let [provider, name] = path else {
        return None;
    };
    if matches!(provider.as_str(), "std" | "core" | "alloc")
        || (provider == "serde_json" && name == "json")
        || (provider == "syn" && name == "Token")
    {
        Some(name)
    } else {
        None
    }
}

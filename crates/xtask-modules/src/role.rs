//! Role: 静的な内側ドキュメントからモジュールの役割宣言を検証する。
use std::path::Path;
use syn::{AttrStyle, Attribute, Expr, Lit, Meta};

/// The module's Role, or the diagnostic that explains why it has none.
pub(crate) fn extract(
    attrs: &[Attribute],
    file: &Path,
    module: &str,
    max_role_chars: usize,
) -> Result<String, String> {
    let context = format!("{} ({module})", file.display());
    let mut declarations = Vec::new();
    for attribute in attrs {
        if !matches!(attribute.style, AttrStyle::Inner(_)) || !attribute.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(value) = &attribute.meta else {
            continue;
        };
        let Expr::Lit(literal) = &value.value else {
            continue;
        };
        let Lit::Str(text) = &literal.lit else {
            continue;
        };
        let text = text.value();
        for line in text.split('\n') {
            let line = line.trim_start();
            if line.starts_with("Role:") {
                let invalid_line = text
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'));
                declarations.push((line.to_string(), invalid_line));
            }
        }
    }
    if declarations.is_empty() {
        return Err(format!(
            "{context}: missing module role; add //! Role: <description>"
        ));
    }
    if declarations.len() != 1 {
        return Err(format!(
            "{context}: duplicate module Role declarations; exactly one is required"
        ));
    }
    let malformed = |reason: &str| {
        Err(format!(
            "{context}: malformed module Role declaration: {reason}"
        ))
    };
    if declarations[0].1 {
        return malformed("Role documentation must be a single line without control characters");
    }
    let Some(description) = declarations[0].0.strip_prefix("Role: ") else {
        return malformed("use Role: followed by a space and a description");
    };
    if description.chars().any(char::is_control) {
        return malformed("description must not contain control characters");
    }
    let description = description.trim();
    if description.is_empty() {
        return malformed("description must not be empty");
    }
    if description.chars().count() > max_role_chars {
        return malformed(&format!(
            "description must be at most {max_role_chars} Unicode characters"
        ));
    }
    Ok(description.to_string())
}

pub(crate) fn validate_limit(file: &Path, max_role_chars: usize) -> Result<(), String> {
    if max_role_chars == 0 {
        return Err(format!(
            "{} (crate): max_role_chars must be greater than 0",
            file.display()
        ));
    }
    Ok(())
}

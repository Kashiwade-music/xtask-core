//! Role: 静的な内側ドキュメントからモジュールの役割宣言を検証する。
use std::path::Path;
use syn::{AttrStyle, Attribute, Expr, Lit, Meta};

/// A `Role:` line of an inner doc, with its place.
struct Declaration {
    /// The line, without its leading whitespace.
    line: String,
    /// Whether it is the first line of the first inner doc.
    first: bool,
    /// Whether its inner doc is not one line: it has a control character or a line separator.
    multiline: bool,
}

/// The module's Role, or the diagnostic that explains why it has none. The Role is the first line
/// of the first inner doc, and it is the only `Role:` line of the module.
pub(crate) fn extract(
    attrs: &[Attribute],
    file: &Path,
    module: &str,
    max_role_chars: usize,
) -> Result<String, String> {
    let context = format!("{} ({module})", file.display());
    let declarations = declarations(attrs);
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
    let declaration = &declarations[0];
    if !declaration.first {
        return Err(format!(
            "{context}: Role must be the first line of the first inner doc"
        ));
    }
    let malformed = |reason: &str| {
        Err(format!(
            "{context}: malformed module Role declaration: {reason}"
        ))
    };
    if declaration.multiline {
        return malformed("Role documentation must be a single line without control characters");
    }
    let Some(description) = declaration.line.strip_prefix("Role: ") else {
        return malformed("use Role: followed by a space and a description");
    };
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

/// The `Role:` lines of the inner docs, in order.
fn declarations(attrs: &[Attribute]) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for (doc, text) in inner_docs(attrs).iter().enumerate() {
        let multiline = text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'));
        for (line_index, line) in text.split('\n').enumerate() {
            let line = line.trim_start();
            if line.starts_with("Role:") {
                declarations.push(Declaration {
                    line: line.to_string(),
                    first: doc == 0 && line_index == 0,
                    multiline,
                });
            }
        }
    }
    declarations
}

/// The texts of the static inner doc attributes, in order.
fn inner_docs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attribute| {
            matches!(attribute.style, AttrStyle::Inner(_)) && attribute.path().is_ident("doc")
        })
        .filter_map(|attribute| {
            let Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let Expr::Lit(literal) = &value.value else {
                return None;
            };
            let Lit::Str(text) = &literal.lit else {
                return None;
            };
            Some(text.value())
        })
        .collect()
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

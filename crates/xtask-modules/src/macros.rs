//! Role: 組込み macro の引数の文法を一つの表にまとめ、引数を解析する。
//! The argument grammars of the built-in macros, and the names their format strings capture.
use proc_macro2::TokenStream;
use std::collections::BTreeSet;
use syn::{
    Expr, Lit, Pat, Token,
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
};

/// The argument grammar of a built-in macro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MacroGrammar {
    /// Comma-separated expressions, or `value; count`. For example `vec!` and `dbg!`.
    Exprs,
    /// `leading` expressions, then an optional format string and its arguments, which are
    /// expressions or `name = expression`. For example `println!` (0) and `assert_eq!` (2).
    Format { leading: usize },
    /// An expression, `,`, a pattern and an optional `if` guard, then, when `trailing_format`,
    /// an optional format string and its arguments. For example `matches!` and `assert_matches!`.
    Matches { trailing_format: bool },
    /// The arguments are not parsed. For example `compile_error!`, whose argument is a string.
    Opaque,
}

/// The grammar of a built-in macro by its name. `None` for a name that the table does not model.
pub(crate) fn grammar(name: &str) -> Option<MacroGrammar> {
    use MacroGrammar::{Exprs, Format, Matches, Opaque};
    Some(match name {
        "format" | "format_args" | "print" | "println" | "eprint" | "eprintln" | "panic"
        | "unreachable" | "todo" | "unimplemented" => Format { leading: 0 },
        "write" | "writeln" | "assert" | "debug_assert" => Format { leading: 1 },
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => Format { leading: 2 },
        "vec" | "dbg" => Exprs,
        "matches" => Matches {
            trailing_format: false,
        },
        "assert_matches" | "debug_assert_matches" => Matches {
            trailing_format: true,
        },
        "compile_error" => Opaque,
        _ => return None,
    })
}

/// The expressions of one macro call, and the names that its format string captures.
#[derive(Default)]
pub(crate) struct Arguments {
    /// The expressions in source order. A named argument (`name = expression`) contributes its value.
    pub(crate) exprs: Vec<Expr>,
    /// The names that a format string takes implicitly from the scope.
    pub(crate) captures: BTreeSet<String>,
}

/// The arguments of a `matches!`-style call.
pub(crate) struct MatchArguments {
    pub(crate) expr: Expr,
    pub(crate) pattern: Pat,
    pub(crate) guard: Option<Expr>,
    /// The format string and its arguments, after the pattern.
    pub(crate) format: Arguments,
}

/// Parses the arguments of a call of a built-in macro by its grammar. `Err` when the tokens do
/// not match the grammar. A `Matches` grammar is parsed by `matches_arguments` instead.
pub(crate) fn arguments(tokens: TokenStream, grammar: MacroGrammar) -> syn::Result<Arguments> {
    match grammar {
        MacroGrammar::Exprs => exprs(tokens),
        MacroGrammar::Format { leading } => format_call(tokens, leading),
        MacroGrammar::Opaque => Ok(Arguments::default()),
        MacroGrammar::Matches { .. } => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "matches-style arguments are parsed by matches_arguments",
        )),
    }
}

/// Parses the arguments of a `matches!`-style call. The format arguments are allowed only when
/// `trailing_format` is set.
pub(crate) fn matches_arguments(
    tokens: TokenStream,
    trailing_format: bool,
) -> syn::Result<MatchArguments> {
    let parser = |input: ParseStream<'_>| -> syn::Result<MatchArguments> {
        let head: MatchHead = input.parse()?;
        let mut tail = Vec::new();
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
            tail = Punctuated::<Expr, Token![,]>::parse_terminated(input)?
                .into_iter()
                .collect();
        }
        if !trailing_format && !tail.is_empty() {
            return Err(input.error("unexpected arguments after the pattern"));
        }
        Ok(MatchArguments {
            expr: head.expr,
            pattern: head.pattern,
            guard: head.guard,
            format: format_tail(tail),
        })
    };
    parser.parse2(tokens)
}

/// The head of a `matches!`-style call: the expression, the pattern and the guard.
struct MatchHead {
    expr: Expr,
    pattern: Pat,
    guard: Option<Expr>,
}

impl Parse for MatchHead {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let pattern = Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self {
            expr,
            pattern,
            guard,
        })
    }
}

fn exprs(tokens: TokenStream) -> syn::Result<Arguments> {
    let exprs = match syn::parse2::<RepeatArgs>(tokens.clone()) {
        Ok(RepeatArgs(value, count)) => vec![value, count],
        Err(_) => items(tokens)?,
    };
    Ok(Arguments {
        exprs,
        captures: BTreeSet::new(),
    })
}

fn format_call(tokens: TokenStream, leading: usize) -> syn::Result<Arguments> {
    let mut items = items(tokens)?;
    let tail = items.split_off(leading.min(items.len()));
    let tail = format_tail(tail);
    items.extend(tail.exprs);
    Ok(Arguments {
        exprs: items,
        captures: tail.captures,
    })
}

/// The comma-separated expressions of the tokens.
fn items(tokens: TokenStream) -> syn::Result<Vec<Expr>> {
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    Ok(parser.parse2(tokens)?.into_iter().collect())
}

/// The arguments after the leading ones: the format string, then its arguments. The names that
/// the explicit arguments name are not captured.
fn format_tail(items: Vec<Expr>) -> Arguments {
    let split = items.into_iter().map(split_named).collect::<Vec<_>>();
    let explicit = split
        .iter()
        .filter_map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    let captures = split
        .first()
        .and_then(|(_, format)| format_text(format))
        .map(|text| implicit_captures(&text, &explicit))
        .unwrap_or_default();
    Arguments {
        exprs: split.into_iter().map(|(_, value)| value).collect(),
        captures,
    }
}

/// Splits `name = value` into the name and the value. Any other expression has no name.
fn split_named(expr: Expr) -> (Option<String>, Expr) {
    match expr {
        Expr::Assign(assign) => match single_identifier(&assign.left) {
            Some(name) => (Some(name), *assign.right),
            None => (None, Expr::Assign(assign)),
        },
        other => (None, other),
    }
}

fn single_identifier(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path)
            if path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1 =>
        {
            Some(path.path.segments[0].ident.to_string())
        }
        _ => None,
    }
}

/// The text of a string literal. `None` for any other expression, such as `concat!`.
fn format_text(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(text) => Some(text.value()),
            _ => None,
        },
        _ => None,
    }
}

/// The names that a format string takes from the scope: the identifiers of its placeholders
/// and the `name$` widths and precisions, without the names of the explicit arguments.
pub(crate) fn implicit_captures(format: &str, explicit: &BTreeSet<String>) -> BTreeSet<String> {
    placeholders(format)
        .iter()
        .flat_map(|placeholder| placeholder_names(placeholder))
        .filter(|name| !explicit.contains(name))
        .collect()
}

/// The text between the braces of each placeholder. `{{` and `}}` are literal text.
fn placeholders(format: &str) -> Vec<String> {
    let chars = format.chars().collect::<Vec<_>>();
    let mut found = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '{' {
            index += 1;
        } else if chars.get(index + 1) == Some(&'{') {
            index += 2;
        } else {
            let end = chars[index + 1..]
                .iter()
                .position(|c| *c == '}')
                .map_or(chars.len(), |offset| index + 1 + offset);
            found.push(chars[index + 1..end].iter().collect());
            index = end + 1;
        }
    }
    found
}

/// The names in one placeholder `argument:spec`.
fn placeholder_names(placeholder: &str) -> Vec<String> {
    let (argument, spec) = placeholder.split_once(':').unwrap_or((placeholder, ""));
    let mut names = dollar_names(spec);
    if is_identifier(argument) {
        names.insert(0, argument.to_owned());
    }
    names
}

/// The identifiers before each `$` in a format spec, such as `w` in `>w$`.
fn dollar_names(spec: &str) -> Vec<String> {
    let chars = spec.chars().collect::<Vec<_>>();
    let mut names = Vec::new();
    for (end, c) in chars.iter().enumerate() {
        if *c != '$' {
            continue;
        }
        let start = chars[..end]
            .iter()
            .rposition(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '#'))
            .map_or(0, |position| position + 1);
        let name = chars[start..end].iter().collect::<String>();
        if is_identifier(&name) {
            names.push(name);
        }
    }
    names
}

/// `[A-Za-z_][A-Za-z0-9_]*`, optionally with the `r#` prefix of a raw identifier.
fn is_identifier(text: &str) -> bool {
    let name = text.strip_prefix("r#").unwrap_or(text);
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The identifier as a single-segment path, for a reference to a captured name.
pub(crate) fn capture_path(name: &str) -> Option<syn::Path> {
    syn::parse_str::<syn::Ident>(name).ok().map(syn::Path::from)
}

/// The arguments of `vec![value; count]`.
struct RepeatArgs(Expr, Expr);

impl Parse for RepeatArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let value = input.parse()?;
        input.parse::<Token![;]>()?;
        let count = input.parse()?;
        Ok(Self(value, count))
    }
}

#[cfg(test)]
mod tests {
    use super::{MacroGrammar, grammar, implicit_captures};
    use std::collections::BTreeSet;

    #[test]
    fn macro_grammar_table() {
        assert_eq!(
            grammar("assert_eq"),
            Some(MacroGrammar::Format { leading: 2 })
        );
        assert_eq!(
            grammar("assert_matches"),
            Some(MacroGrammar::Matches {
                trailing_format: true
            })
        );
        assert_eq!(grammar("compile_error"), Some(MacroGrammar::Opaque));
        assert_eq!(grammar("json"), None);
    }

    #[test]
    fn format_capture_rules() {
        let explicit = BTreeSet::from(["x".to_owned()]);
        assert_eq!(
            implicit_captures("{{N}} {x:>w$.p$} {0}", &explicit),
            BTreeSet::from(["w".to_owned(), "p".to_owned()])
        );
        assert_eq!(
            implicit_captures("{:.*} {r#type:?} {:0$}", &BTreeSet::new()),
            BTreeSet::from(["r#type".to_owned()])
        );
    }
}

//! Role: Rustの字句と物理行からコメント・文字列内部を除くコード行数を数える。
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::borrow::Cow;
use std::collections::BTreeSet;

/// Count physical lines containing Rust code after removing comments and literal
/// contents. Literal delimiters count; interior literal lines do not.
///
/// This uses the Rust lexer in `proc-macro2`, preserving physical span locations
/// even for escaped newlines. Source that cannot be tokenized is an error, so a
/// malformed file is never counted by a guess.
pub fn code_line_count(source: &str) -> Result<usize, String> {
    let source = without_file_preamble(source);
    let tokens: TokenStream = source
        .parse()
        .map_err(|error| format!("cannot tokenize Rust source: {error}"))?;
    let mut lines = BTreeSet::new();
    mark_tokens(&source, tokens, &mut lines);
    Ok(lines.len())
}

fn mark_tokens(source: &str, tokens: TokenStream, lines: &mut BTreeSet<usize>) {
    for token in tokens {
        // Rust lexers expand doc comments into attribute tokens. All generated
        // tokens retain the original comment span, which distinguishes them from
        // explicit #[doc = "..."] attributes (actual Rust code).
        if is_comment_span(source, token.span()) {
            continue;
        }
        match token {
            TokenTree::Group(group) => {
                if group.delimiter() != Delimiter::None {
                    lines.insert(group.span_open().start().line);
                    lines.insert(group.span_close().start().line);
                }
                mark_tokens(source, group.stream(), lines);
            }
            TokenTree::Literal(literal) => {
                // A valid literal's opener and closer are on its first and last
                // physical lines, including raw hashes, byte/C prefixes, suffixes,
                // and escaped physical newlines. No interior content is code.
                lines.insert(literal.span().start().line);
                lines.insert(literal.span().end().line);
            }
            token => {
                lines.insert(token.span().start().line);
            }
        }
    }
}

fn is_comment_span(source: &str, span: Span) -> bool {
    // Only the first two bytes of the token are needed, so read them from the
    // parsed text instead of copying the token (or a whole group) into a String.
    let start = span.byte_range().start;
    source
        .as_bytes()
        .get(start..start + 2)
        .is_some_and(|prefix| prefix == b"//" || prefix == b"/*")
}

fn without_file_preamble(source: &str) -> Cow<'_, str> {
    // A UTF-8 BOM and Unix interpreter directive are file-level constructs,
    // rather than token-stream syntax. Remove them without removing any newline.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    if source.starts_with("#!") && !starts_inner_attribute(&source[2..]) {
        Cow::Owned(
            source
                .find('\n')
                .map_or_else(String::new, |end| format!("\n{}", &source[end + 1..])),
        )
    } else {
        Cow::Borrowed(source)
    }
}

/// The whitespace of the Rust lexer. Unicode whitespace such as U+00A0 is not
/// included, so it does not hide the start of an inner attribute.
fn is_rust_whitespace(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\u{0b}'
            | '\u{0c}'
            | '\u{85}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

fn starts_inner_attribute(mut rest: &str) -> bool {
    // Whitespace and ordinary comments may appear between `#!` and `[`.
    // Doc comments are tokens, so they do not participate in this trivia skip.
    loop {
        rest = rest.trim_start_matches(is_rust_whitespace);
        if rest.starts_with("//")
            && (!rest.starts_with("///") || rest.starts_with("////"))
            && !rest.starts_with("//!")
        {
            rest = rest.find('\n').map_or("", |end| &rest[end + 1..]);
        } else if rest.starts_with("/**/") {
            rest = &rest[4..];
        } else if rest.starts_with("/*")
            && (!rest.starts_with("/**") || rest.starts_with("/***"))
            && !rest.starts_with("/*!")
        {
            let mut depth = 1usize;
            let mut position = 2;
            let bytes = rest.as_bytes();
            while depth > 0 && position + 1 < bytes.len() {
                match &bytes[position..position + 2] {
                    b"/*" => {
                        depth += 1;
                        position += 2;
                    }
                    b"*/" => {
                        depth -= 1;
                        position += 2;
                    }
                    _ => position += 1,
                }
            }
            if depth != 0 {
                return false;
            }
            rest = &rest[position..];
        } else {
            return rest.starts_with('[');
        }
    }
}

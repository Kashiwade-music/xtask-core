//! Role: Rustの字句と物理行からコメント・文字列内部を除くコード行数を数える。
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::collections::BTreeSet;

/// Count physical lines containing Rust code after removing comments and literal
/// contents. Literal delimiters count; interior literal lines do not.
///
/// This uses the Rust lexer in `proc-macro2`, preserving physical span locations
/// even for escaped newlines. If the source cannot be tokenized, this convenience
/// function conservatively counts every nonblank line. Policy checks use the
/// fallible variant instead and report the malformed input as an error.
pub fn code_line_count(source: &str) -> usize {
    try_code_line_count(source).unwrap_or_else(|_| {
        source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    })
}

pub(crate) fn try_code_line_count(source: &str) -> Result<usize, String> {
    let source = without_file_preamble(source);
    let tokens: TokenStream = source
        .parse()
        .map_err(|error| format!("cannot tokenize Rust source: {error}"))?;
    let mut lines = BTreeSet::new();
    mark_tokens(tokens, &mut lines);
    Ok(lines.len())
}

fn mark_tokens(tokens: TokenStream, lines: &mut BTreeSet<usize>) {
    for token in tokens {
        // Rust lexers expand doc comments into attribute tokens. All generated
        // tokens retain the original comment span, which distinguishes them from
        // explicit #[doc = "..."] attributes (actual Rust code).
        if is_comment_span(token.span()) {
            continue;
        }
        match token {
            TokenTree::Group(group) => {
                if group.delimiter() != Delimiter::None {
                    lines.insert(group.span_open().start().line);
                    lines.insert(group.span_close().start().line);
                }
                mark_tokens(group.stream(), lines);
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

fn is_comment_span(span: Span) -> bool {
    span.source_text()
        .is_some_and(|text| text.starts_with("//") || text.starts_with("/*"))
}

fn without_file_preamble(source: &str) -> String {
    // A UTF-8 BOM and Unix interpreter directive are file-level constructs,
    // rather than token-stream syntax. Remove them without removing any newline.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    if source.starts_with("#!") && !starts_inner_attribute(&source[2..]) {
        source
            .find('\n')
            .map_or_else(String::new, |end| format!("\n{}", &source[end + 1..]))
    } else {
        source.to_owned()
    }
}

fn starts_inner_attribute(mut rest: &str) -> bool {
    // Whitespace and ordinary comments may appear between `#!` and `[`.
    // Doc comments are tokens, so they do not participate in this trivia skip.
    loop {
        rest = rest.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '\u{200e}' | '\u{200f}')
        });
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

//! serde_json::json! grammar: object/array containers plus Rust expression values.
use proc_macro2::TokenStream;
use syn::{
    Expr, Token,
    parse::{ParseStream, Parser},
};

pub(crate) fn expressions(tokens: TokenStream) -> syn::Result<Vec<Expr>> {
    let parser = |input: ParseStream<'_>| {
        let mut expressions = Vec::new();
        value(input, &mut expressions)?;
        Ok(expressions)
    };
    parser.parse2(tokens)
}

fn value(input: ParseStream<'_>, expressions: &mut Vec<Expr>) -> syn::Result<()> {
    if input.peek(syn::token::Brace) || input.peek(syn::token::Bracket) {
        let group: proc_macro2::Group = input.parse()?;
        let object = group.delimiter() == proc_macro2::Delimiter::Brace;
        let parser = |input: ParseStream<'_>| {
            while !input.is_empty() {
                if object {
                    expressions.push(input.parse::<Expr>()?);
                    input.parse::<Token![:]>()?;
                }
                value(input, expressions)?;
                if input.is_empty() {
                    break;
                }
                input.parse::<Token![,]>()?;
            }
            Ok(())
        };
        parser.parse2(group.stream())
    } else {
        expressions.push(input.parse()?);
        Ok(())
    }
}

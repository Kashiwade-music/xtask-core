//! Attribute access that does not inspect macro token trees.
use syn::{Attribute, Expr};

pub(crate) fn expr_attrs(expr: &Expr) -> &[Attribute] {
    match expr {
        Expr::Array(i) => &i.attrs,
        Expr::Assign(i) => &i.attrs,
        Expr::Async(i) => &i.attrs,
        Expr::Await(i) => &i.attrs,
        Expr::Binary(i) => &i.attrs,
        Expr::Block(i) => &i.attrs,
        Expr::Break(i) => &i.attrs,
        Expr::Call(i) => &i.attrs,
        Expr::Cast(i) => &i.attrs,
        Expr::Closure(i) => &i.attrs,
        Expr::Const(i) => &i.attrs,
        Expr::Continue(i) => &i.attrs,
        Expr::Field(i) => &i.attrs,
        Expr::ForLoop(i) => &i.attrs,
        Expr::Group(i) => &i.attrs,
        Expr::If(i) => &i.attrs,
        Expr::Index(i) => &i.attrs,
        Expr::Infer(i) => &i.attrs,
        Expr::Let(i) => &i.attrs,
        Expr::Lit(i) => &i.attrs,
        Expr::Loop(i) => &i.attrs,
        Expr::Macro(i) => &i.attrs,
        Expr::Match(i) => &i.attrs,
        Expr::MethodCall(i) => &i.attrs,
        Expr::Paren(i) => &i.attrs,
        Expr::Path(i) => &i.attrs,
        Expr::Range(i) => &i.attrs,
        Expr::RawAddr(i) => &i.attrs,
        Expr::Reference(i) => &i.attrs,
        Expr::Repeat(i) => &i.attrs,
        Expr::Return(i) => &i.attrs,
        Expr::Struct(i) => &i.attrs,
        Expr::Try(i) => &i.attrs,
        Expr::TryBlock(i) => &i.attrs,
        Expr::Tuple(i) => &i.attrs,
        Expr::Unary(i) => &i.attrs,
        Expr::Unsafe(i) => &i.attrs,
        Expr::While(i) => &i.attrs,
        Expr::Yield(i) => &i.attrs,
        _ => &[],
    }
}

pub(crate) fn impl_attrs(item: &syn::ImplItem) -> &[Attribute] {
    match item {
        syn::ImplItem::Const(i) => &i.attrs,
        syn::ImplItem::Fn(i) => &i.attrs,
        syn::ImplItem::Type(i) => &i.attrs,
        syn::ImplItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

pub(crate) fn trait_attrs(item: &syn::TraitItem) -> &[Attribute] {
    match item {
        syn::TraitItem::Const(i) => &i.attrs,
        syn::TraitItem::Fn(i) => &i.attrs,
        syn::TraitItem::Type(i) => &i.attrs,
        syn::TraitItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

pub(crate) fn foreign_attrs(item: &syn::ForeignItem) -> &[Attribute] {
    match item {
        syn::ForeignItem::Fn(i) => &i.attrs,
        syn::ForeignItem::Static(i) => &i.attrs,
        syn::ForeignItem::Type(i) => &i.attrs,
        syn::ForeignItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

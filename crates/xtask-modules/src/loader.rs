use crate::{AnalysisOptions, Module, cfg};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use syn::{Attribute, Item, Meta};

pub(crate) struct Source {
    pub module: Module,
    pub items: Vec<Item>,
    pub attrs: Vec<Attribute>,
}

pub(crate) fn attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

pub(crate) fn load(root: &Path, options: &AnalysisOptions) -> Result<Vec<Source>, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut loader = Loader {
        sources: Vec::new(),
        stack: BTreeSet::new(),
        options,
    };
    let base = root
        .parent()
        .ok_or("root file has no parent")?
        .to_path_buf();
    loader.file(root, "crate".into(), None, base)?;
    Ok(loader.sources)
}

struct Loader<'a> {
    sources: Vec<Source>,
    stack: BTreeSet<PathBuf>,
    options: &'a AnalysisOptions,
}

struct Context {
    file: PathBuf,
    id: String,
    parent: Option<String>,
    base: PathBuf,
    inline: bool,
}

impl Loader<'_> {
    fn file(
        &mut self,
        file: PathBuf,
        id: String,
        parent: Option<String>,
        base: PathBuf,
    ) -> Result<(), String> {
        let canonical = fs::canonicalize(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if !self.stack.insert(canonical.clone()) {
            return Err(format!("recursive module file: {}", file.display()));
        }
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let ast = syn::parse_file(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        if cfg::enabled(&ast.attrs, self.options)? {
            self.items(
                ast.items,
                ast.attrs,
                Context {
                    file,
                    id,
                    parent,
                    base,
                    inline: false,
                },
            )?;
        }
        self.stack.remove(&canonical);
        Ok(())
    }

    fn items(
        &mut self,
        items: Vec<Item>,
        attributes: Vec<Attribute>,
        context: Context,
    ) -> Result<(), String> {
        let Context {
            file,
            id,
            parent,
            base,
            inline,
        } = context;
        let mut active = Vec::new();
        for item in items {
            if !cfg::enabled(attrs(&item), self.options)? {
                continue;
            }
            if let Item::Mod(module) = &item {
                let child_id = format!("{id}::{}", module.ident);
                let mut path = None;
                for attr in cfg::effective(&module.attrs, self.options)? {
                    if attr.path().is_ident("path") {
                        let Meta::NameValue(value) = attr else {
                            return Err("invalid path attribute".into());
                        };
                        let syn::Expr::Lit(lit) = value.value else {
                            return Err("nonliteral path attribute".into());
                        };
                        let syn::Lit::Str(value) = lit.lit else {
                            return Err("nonstring path attribute".into());
                        };
                        path = Some(value.value());
                    }
                }
                let attribute_base = if inline {
                    &base
                } else {
                    file.parent().unwrap()
                };
                if let Some((_, contents)) = &module.content {
                    let child_base = if let Some(path) = &path {
                        attribute_base.join(path)
                    } else {
                        base.join(module.ident.to_string())
                    };
                    self.items(
                        contents.clone(),
                        module.attrs.clone(),
                        Context {
                            file: file.clone(),
                            id: child_id,
                            parent: Some(id.clone()),
                            base: child_base,
                            inline: true,
                        },
                    )?;
                } else {
                    let explicit_path = path.is_some();
                    let child_file = if let Some(path) = path {
                        attribute_base.join(path)
                    } else {
                        let flat = base.join(format!("{}.rs", module.ident));
                        let nested = base.join(module.ident.to_string()).join("mod.rs");
                        match (flat.exists(), nested.exists()) {
                            (true, false) => flat,
                            (false, true) => nested,
                            (true, true) => {
                                return Err(format!("ambiguous module file for {child_id}"));
                            }
                            _ => {
                                return Err(format!(
                                    "module file not found for {child_id} under {}",
                                    base.display()
                                ));
                            }
                        }
                    };
                    let child_base =
                        if explicit_path || child_file.file_name().is_some_and(|n| n == "mod.rs") {
                            child_file.parent().unwrap().to_path_buf()
                        } else {
                            child_file.with_extension("")
                        };
                    self.file(child_file, child_id, Some(id.clone()), child_base)?;
                }
            }
            active.push(item);
        }
        self.sources.push(Source {
            module: Module { id, file, parent },
            items: active,
            attrs: attributes,
        });
        Ok(())
    }
}

//! Role: cfgとpath属性に従ってモジュールのソースと有効な構文を読み込む。
use crate::{
    ast, cfg,
    model::{Env, Module, ModuleKind},
    role,
};
use std::{
    collections::BTreeMap,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};
use syn::{Attribute, Item, ItemMod, Meta, ext::IdentExt, spanned::Spanned};

/// Parsed files of one analysis, shared by its configurations. A failure is kept too,
/// so a broken file is read and reported once.
pub(crate) type ParseCache = BTreeMap<PathBuf, Result<Rc<syn::File>, String>>;

/// One module and the place of its items. The items stay in the parsed file; a source
/// names them by index, so nothing is copied.
pub(crate) struct Source {
    pub(crate) module: Module,
    file: Rc<syn::File>,
    /// Indices of the inline modules from the file root; empty for a file module.
    scope: Vec<usize>,
    /// The module's own attributes after cfg_attr expansion.
    attributes: Vec<Meta>,
    /// The active items: the index of each in the scope, and its attributes after cfg_attr expansion.
    active: Vec<(usize, Vec<Meta>)>,
}

impl Source {
    /// The active items of the module, each with its attributes after cfg_attr expansion.
    pub(crate) fn items(&self) -> Vec<(&Item, &[Meta])> {
        let (_, items) = scope_of(&self.file, &self.scope);
        self.active
            .iter()
            .map(|(index, attributes)| (&items[*index], attributes.as_slice()))
            .collect()
    }

    /// The module's own attributes after cfg_attr expansion.
    pub(crate) fn attributes(&self) -> &[Meta] {
        &self.attributes
    }
}

/// The modules of one configuration, with the diagnostics of their loading.
pub(crate) struct Loaded {
    pub(crate) sources: Vec<Source>,
    pub(crate) diagnostics: Vec<String>,
}

/// The module path of a child, e.g. `crate::a::b`.
pub(crate) fn child_id(parent: &str, name: &impl Display) -> String {
    format!("{parent}::{name}")
}

/// Loads the crate root under one configuration. Parsed files come from `cache`.
pub(crate) fn load(root: &Path, env: Env<'_>, cache: &mut ParseCache) -> Result<Loaded, String> {
    let root = xtask_source::normalize(root)?;
    let base = root
        .parent()
        .ok_or("root file has no parent")?
        .to_path_buf();
    let mut loader = Loader {
        env,
        cache,
        pending: Vec::new(),
        diagnostics: Vec::new(),
    };
    loader.file(
        root,
        "crate".into(),
        None,
        DirOwnership::ModRs(base),
        &mut Vec::new(),
    )?;
    Ok(Loaded {
        sources: finish(loader.pending),
        diagnostics: loader.diagnostics,
    })
}

/// Which directory a module's children are looked up in.
enum DirOwnership {
    /// `mod.rs`, the crate root, or a file named by a `path` attribute: children sit beside it.
    ModRs(PathBuf),
    /// `a.rs`: children sit in `a/`.
    NonModRs(PathBuf),
    /// An inline module: its children are looked up in the directory of its enclosing file.
    Inline(PathBuf),
}

impl DirOwnership {
    fn children(&self) -> &Path {
        match self {
            Self::ModRs(dir) | Self::NonModRs(dir) | Self::Inline(dir) => dir,
        }
    }
}

/// Where a module's items are: its file, the inline-module path inside that file, and the
/// module's own attributes after cfg_attr expansion.
struct Site {
    file_path: PathBuf,
    file: Rc<syn::File>,
    scope: Vec<usize>,
    attributes: Vec<Meta>,
}

/// A `mod` item that is active under the configuration.
struct Declaration<'a> {
    index: usize,
    module: &'a ItemMod,
    attributes: &'a [Meta],
}

struct Pending {
    parent: Option<String>,
    source: Source,
}

struct Loader<'a> {
    env: Env<'a>,
    cache: &'a mut ParseCache,
    pending: Vec<Pending>,
    diagnostics: Vec<String>,
}

impl Loader<'_> {
    /// Loads the file module at `path`, which is already normalized. Returns whether the
    /// module is active: a file disabled by its own `#![cfg]` yields no module.
    fn file(
        &mut self,
        path: PathBuf,
        id: String,
        parent: Option<String>,
        owner: DirOwnership,
        stack: &mut Vec<PathBuf>,
    ) -> Result<bool, String> {
        stack.push(path.clone());
        let result = self.file_body(path, id, parent, owner, stack);
        stack.pop();
        result
    }

    fn file_body(
        &mut self,
        file_path: PathBuf,
        id: String,
        parent: Option<String>,
        owner: DirOwnership,
        stack: &mut Vec<PathBuf>,
    ) -> Result<bool, String> {
        let file = parse(self.cache, &file_path)?;
        let line = file
            .attrs
            .first()
            .map_or(1, |attr| attr.span().start().line);
        let Some(attributes) = self.expanded(&file_path, line, &file.attrs) else {
            return Ok(false);
        };
        let site = Site {
            file_path,
            file,
            scope: Vec::new(),
            attributes,
        };
        self.module(&site, id, parent, &owner, ModuleKind::File, stack)?;
        Ok(true)
    }

    /// Expands the cfg_attr of one item's or file's attributes, and evaluates its cfg. Returns
    /// the expanded attributes when the item is enabled. An invalid predicate is one diagnostic,
    /// and the item is treated as disabled.
    fn expanded(
        &mut self,
        file_path: &Path,
        line: usize,
        attrs: &[Attribute],
    ) -> Option<Vec<Meta>> {
        let evaluated = cfg::effective(attrs, self.env).and_then(|metas| {
            let enabled = cfg::enabled_metas(&metas, self.env)?;
            Ok(enabled.then_some(metas))
        });
        match evaluated {
            Ok(metas) => metas,
            Err(error) => {
                self.diagnostics.push(located(file_path, line, &error));
                None
            }
        }
    }

    fn module(
        &mut self,
        site: &Site,
        id: String,
        parent: Option<String>,
        owner: &DirOwnership,
        kind: ModuleKind,
        stack: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        let (attributes, _) = scope_of(&site.file, &site.scope);
        let role = role::extract(
            attributes,
            &site.file_path,
            &id,
            self.env.settings.max_role_chars,
        );
        if let Err(message) = &role {
            self.diagnostics.push(message.clone());
        }
        let active = self.scan(site, &id, owner, stack)?;
        self.pending.push(Pending {
            parent,
            source: Source {
                module: Module {
                    id,
                    file: site.file_path.clone(),
                    parent: None,
                    kind,
                    role,
                },
                file: Rc::clone(&site.file),
                scope: site.scope.clone(),
                attributes: site.attributes.clone(),
                active,
            },
        });
        Ok(())
    }

    /// Lists the active items of a module, with their expanded attributes. A `mod` item is
    /// active only when its module loads.
    fn scan(
        &mut self,
        site: &Site,
        id: &str,
        owner: &DirOwnership,
        stack: &mut Vec<PathBuf>,
    ) -> Result<Vec<(usize, Vec<Meta>)>, String> {
        let (_, items) = scope_of(&site.file, &site.scope);
        let mut active = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let line = item.span().start().line;
            let Some(attributes) = self.expanded(&site.file_path, line, ast::attrs(item)) else {
                continue;
            };
            let loaded = match item {
                Item::Mod(module) => {
                    let declaration = Declaration {
                        index,
                        module,
                        attributes: &attributes,
                    };
                    self.child(site, declaration, id, owner, stack)?
                }
                _ => true,
            };
            if loaded {
                active.push((index, attributes));
            }
        }
        Ok(active)
    }

    /// Loads the module that an active `mod` item declares. Returns whether it is active.
    fn child(
        &mut self,
        site: &Site,
        declaration: Declaration<'_>,
        id: &str,
        owner: &DirOwnership,
        stack: &mut Vec<PathBuf>,
    ) -> Result<bool, String> {
        let Declaration {
            index,
            module,
            attributes,
        } = declaration;
        // A raw identifier names its module without the `r#` prefix, in the id and in the file name.
        let name = module.ident.unraw();
        let child_id = child_id(id, &name);
        let line = module.ident.span().start().line;
        let path = path_attribute(&site.file_path, line, attributes)?;
        let attribute_base = attribute_base(owner, &site.file_path);
        if module.content.is_some() {
            let directory = match &path {
                Some(path) => attribute_base.join(path),
                None => owner.children().join(name.to_string()),
            };
            let mut scope = site.scope.clone();
            scope.push(index);
            let inline = Site {
                file_path: site.file_path.clone(),
                file: Rc::clone(&site.file),
                scope,
                attributes: attributes.to_vec(),
            };
            let owner = DirOwnership::Inline(directory);
            self.module(
                &inline,
                child_id,
                Some(id.to_string()),
                &owner,
                ModuleKind::Inline,
                stack,
            )?;
            return Ok(true);
        }
        let explicit = path.is_some();
        let child_file = match path {
            Some(path) => attribute_base.join(path),
            None => sibling_file(&site.file_path, owner.children(), &name, &child_id, line)?,
        };
        // Files are keyed by their normalized paths, so one file has one stack entry and one parse.
        let child_file = xtask_source::normalize(&child_file)
            .map_err(|error| located(&site.file_path, line, &error))?;
        if stack.contains(&child_file) {
            return Err(located(
                &site.file_path,
                line,
                &format!("recursive module file: {}", child_file.display()),
            ));
        }
        let child_owner = if explicit || child_file.file_name().is_some_and(|name| name == "mod.rs")
        {
            DirOwnership::ModRs(child_file.parent().unwrap().to_path_buf())
        } else {
            DirOwnership::NonModRs(child_file.with_extension(""))
        };
        self.file(
            child_file,
            child_id,
            Some(id.to_string()),
            child_owner,
            stack,
        )
    }
}

/// The `path` attribute of a `mod` item, read from its expanded attributes.
fn path_attribute(
    file_path: &Path,
    line: usize,
    attributes: &[Meta],
) -> Result<Option<String>, String> {
    let mut path = None;
    for meta in attributes {
        if !meta.path().is_ident("path") {
            continue;
        }
        let Meta::NameValue(value) = meta else {
            return Err(located(file_path, line, "invalid path attribute"));
        };
        let syn::Expr::Lit(lit) = &value.value else {
            return Err(located(file_path, line, "nonliteral path attribute"));
        };
        let syn::Lit::Str(value) = &lit.lit else {
            return Err(located(file_path, line, "nonstring path attribute"));
        };
        path = Some(value.value());
    }
    Ok(path)
}

/// The file of a `mod` item without a `path` attribute: `name.rs` or `name/mod.rs`.
fn sibling_file(
    file_path: &Path,
    base: &Path,
    ident: &syn::Ident,
    child_id: &str,
    line: usize,
) -> Result<PathBuf, String> {
    let flat = base.join(format!("{ident}.rs"));
    let nested = base.join(ident.to_string()).join("mod.rs");
    match (flat.exists(), nested.exists()) {
        (true, false) => Ok(flat),
        (false, true) => Ok(nested),
        (true, true) => Err(located(
            file_path,
            line,
            &format!("ambiguous module file for {child_id}"),
        )),
        _ => Err(located(
            file_path,
            line,
            &format!(
                "module file not found for {child_id} under {}",
                base.display()
            ),
        )),
    }
}

/// The directory that a `path` attribute is relative to.
fn attribute_base(owner: &DirOwnership, file_path: &Path) -> PathBuf {
    match owner {
        DirOwnership::Inline(directory) => directory.clone(),
        DirOwnership::ModRs(_) | DirOwnership::NonModRs(_) => {
            file_path.parent().unwrap().to_path_buf()
        }
    }
}

/// A loader error at a place in a file, in the form `{file}:{line}: {message}`.
pub(crate) fn located(file_path: &Path, line: usize, message: &str) -> String {
    format!("{}:{line}: {message}", file_path.display())
}

fn parse(cache: &mut ParseCache, path: &Path) -> Result<Rc<syn::File>, String> {
    cache
        .entry(path.to_path_buf())
        .or_insert_with(|| read_and_parse(path))
        .clone()
}

fn read_and_parse(path: &Path) -> Result<Rc<syn::File>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    syn::parse_file(&text)
        .map(Rc::new)
        .map_err(|e| located(path, e.span().start().line, &e.to_string()))
}

/// The attributes and the items of the module at `scope` (the file root when it is empty).
fn scope_of<'a>(file: &'a syn::File, scope: &[usize]) -> (&'a [Attribute], &'a [Item]) {
    let mut attributes: &'a [Attribute] = &file.attrs;
    let mut items: &'a [Item] = &file.items;
    for &index in scope {
        let Item::Mod(module) = &items[index] else {
            unreachable!("an inline scope names a module")
        };
        let Some((_, contents)) = &module.content else {
            unreachable!("an inline scope names an inline module")
        };
        attributes = &module.attrs;
        items = contents.as_slice();
    }
    (attributes, items)
}

/// Orders the modules by id, which puts every parent before its children, and gives each
/// parent its index. No other code reorders modules.
fn finish(mut pending: Vec<Pending>) -> Vec<Source> {
    pending.sort_by(|a, b| a.source.module.id.cmp(&b.source.module.id));
    let index: BTreeMap<String, usize> = pending
        .iter()
        .enumerate()
        .map(|(index, pending)| (pending.source.module.id.clone(), index))
        .collect();
    pending
        .into_iter()
        .map(|pending| {
            let Pending { parent, mut source } = pending;
            source.module.parent = parent.map(|id| index[&id]);
            source
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Role: ソースの読み込みと解析結果の cache（失敗も保存）の試験。
    use super::*;
    use crate::model::Settings;
    use std::collections::BTreeSet;

    fn settings() -> Settings {
        Settings {
            features: BTreeSet::new(),
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
            trusted_macros: BTreeSet::new(),
            trusted_globs: BTreeSet::new(),
            generated_type_attributes: BTreeSet::new(),
            max_role_chars: 80,
        }
    }

    #[test]
    fn parse_cache_keeps_failures() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("lib.rs");
        fs::write(&root, "//! Role: Root.\nmod b;\n").unwrap();
        fs::write(directory.path().join("b.rs"), "fn broken( {\n").unwrap();
        let settings = settings();
        let plain = BTreeSet::new();
        let test = BTreeSet::from(["test".to_string()]);
        let mut cache = ParseCache::new();
        let first = load(
            &root,
            Env {
                settings: &settings,
                cfg: &plain,
            },
            &mut cache,
        )
        .err()
        .unwrap();
        let second = load(
            &root,
            Env {
                settings: &settings,
                cfg: &test,
            },
            &mut cache,
        )
        .err()
        .unwrap();
        assert_eq!(first, second);
        let entries = cache
            .iter()
            .filter(|(path, _)| path.ends_with("b.rs"))
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.is_err());
    }
}

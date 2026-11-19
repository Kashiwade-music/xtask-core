//! Role: Share the temporary fixtures and the options of the module analysis tests.
use std::{collections::BTreeSet, fs, path::Path};
use xtask_modules::{Configuration, ModuleGraph, Settings, analyze};

/// The options of one test: the settings of the package and one configuration.
#[derive(Clone)]
pub struct Options {
    pub cfg: BTreeSet<String>,
    pub features: BTreeSet<String>,
    pub trusted_attributes: BTreeSet<String>,
    pub trusted_derives: BTreeSet<String>,
    pub trusted_macros: BTreeSet<String>,
    pub trusted_globs: BTreeSet<String>,
    pub max_role_chars: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cfg: BTreeSet::new(),
            features: BTreeSet::new(),
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
            trusted_macros: BTreeSet::new(),
            trusted_globs: BTreeSet::new(),
            max_role_chars: 80,
        }
    }
}

impl Options {
    /// The package settings part of these options.
    pub fn settings(&self) -> Settings {
        Settings {
            features: self.features.clone(),
            trusted_attributes: self.trusted_attributes.clone(),
            trusted_derives: self.trusted_derives.clone(),
            trusted_macros: self.trusted_macros.clone(),
            trusted_globs: self.trusted_globs.clone(),
            max_role_chars: self.max_role_chars,
        }
    }

    /// Analyzes `root` under this configuration.
    pub fn analyze(&self, root: &Path) -> Result<ModuleGraph, String> {
        let configuration = Configuration {
            cfg: self.cfg.clone(),
        };
        analyze(root, &self.settings(), &[configuration])
            .pop()
            .expect("one configuration gives one result")
    }
}

/// Writes the files below a new temporary directory, verbatim.
pub fn write_fixture<S: AsRef<str>>(files: &[(&str, S)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, source) in files {
        let path = directory.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source.as_ref()).unwrap();
    }
    directory
}

pub fn graph(files: &[(&str, &str)], options: Options) -> ModuleGraph {
    let files = files
        .iter()
        .map(|(path, source)| (*path, with_fixture_roles(source)))
        .collect::<Vec<_>>();
    let directory = write_fixture(&files);
    options.analyze(&directory.path().join("lib.rs")).unwrap()
}

// Existing reference-resolution fixtures need valid module annotations so their
// diagnostic assertions remain focused on reference analysis. This helper is
// deliberately local to these fixtures; role-policy tests write source verbatim.
pub fn with_fixture_roles(source: &str) -> String {
    use syn::visit::{self, Visit};

    struct InlineModules(Vec<proc_macro2::LineColumn>);
    impl<'ast> Visit<'ast> for InlineModules {
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            if let Some((brace, _)) = &module.content {
                self.0.push(brace.span.open().end());
            }
            visit::visit_item_mod(self, module);
        }
    }
    let Ok(file) = syn::parse_file(source) else {
        // Malformed-source regression fixtures must retain their original text.
        return source.to_owned();
    };
    let mut modules = InlineModules(Vec::new());
    modules.visit_file(&file);
    let mut positions: Vec<_> = modules
        .0
        .iter()
        .map(|location| {
            let start: usize = source
                .split_inclusive('\n')
                .take(location.line - 1)
                .map(str::len)
                .sum();
            start
                + source[start..]
                    .char_indices()
                    .nth(location.column)
                    .map_or(source.len() - start, |(offset, _)| offset)
        })
        .collect();
    positions.sort_unstable();
    let mut annotated = source.to_owned();
    for position in positions.into_iter().rev() {
        annotated.insert_str(
            position,
            "\n//! Role: Exercise module reference analysis.\n",
        );
    }
    format!("//! Role: Exercise module reference analysis.\n{annotated}")
}

pub fn edges(graph: &ModuleGraph) -> BTreeSet<(&str, &str)> {
    graph
        .edges
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str()))
        .collect()
}

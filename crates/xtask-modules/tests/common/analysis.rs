//! Role: analysis 系の試験だけが使う helper（graph、with_fixture_roles、edges）。

use crate::common::{Options, write_fixture};
use std::collections::BTreeSet;
use xtask_modules::ModuleGraph;

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

//! Role: 木（crate・target・module・要素）の行の組立て、絞込み、開く操作の決定、sidebar の画面。
use std::collections::{BTreeMap, BTreeSet};

use kivact::interaction::TreeNode;
use kivact::prelude::{
    Element, Handled, IntoView, Key, KeyEvent, column, effect, memo, scroll, signal, style,
    text_input,
};
use kivact::ui::tree;
use xtask_archive::{Content, Diagram, DiagramKind, Mode, Scene};

use crate::navigate;
use crate::state::{AppState, Document, Loaded};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TreeKey {
    Package(u32),
    Diagram(u32),
    Item { diagram: u32, item: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: TreeKey,
    pub parent: Option<TreeKey>,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open(usize),
    Reveal(usize, u32),
}

/// 木と検索の欄。単クリックは選択だけで、Enter かダブルクリックで開く。
pub fn sidebar(state: AppState) -> impl IntoView {
    let selected = signal(None::<TreeKey>);
    let expanded = signal(Vec::<TreeKey>::new());
    let shown = memo(move || {
        let search = state.search.get();
        match state.document.get() {
            Document::Loaded { loaded, .. } => rows(&loaded, &search),
            _ => Vec::new(),
        }
    });
    let nodes = memo(move || {
        shown.with(|rows| {
            rows.iter()
                .map(|row| TreeNode::new(row.key, row.parent, row.label.clone()))
                .collect::<Vec<_>>()
        })
    });
    // 絞込み中は、残した行をすべて展開する。
    effect(move || {
        if state.search.with(String::is_empty) {
            return;
        }
        let keys = shown.with(|rows| branch_keys(rows));
        expanded.set(keys);
    });
    column((
        text_input(state.search)
            .label("Search")
            .placeholder("Filter crates and modules")
            .on_key(|event: KeyEvent| {
                if event.key() == Key::Backspace {
                    Handled::Yes
                } else {
                    Handled::No
                }
            }),
        scroll(
            tree("Crates and modules", nodes, selected)
                .expanded(expanded)
                .on_activate(move |key: TreeKey| activate(state, key)),
        )
        .style(style().flex_grow(1).min_height(0)),
    ))
}

/// Enter かダブルクリックで呼ばれる。文書が読めていなければ何もしない。
fn activate(state: AppState, key: TreeKey) {
    let Document::Loaded { loaded, .. } = state.document.get_untracked() else {
        return;
    };
    match action(&loaded, key) {
        Some(Action::Open(diagram)) => navigate::go(state, diagram),
        Some(Action::Reveal(diagram, item)) => navigate::reveal(state, diagram, item),
        None => {}
    }
}

/// 木の行。package（crate）の下に、その package の module 図、図の下に要素（Drawn の図だけ）。
/// `search` が空でなければ、名前（小文字）が含む行と、その祖先だけを残す。
pub fn rows(loaded: &Loaded, search: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for (index, name) in packages(loaded).iter().enumerate() {
        let key = TreeKey::Package(index as u32);
        out.push(Row {
            key,
            parent: None,
            label: name.clone(),
        });
        for (diagram, entry) in loaded.archive.diagrams.iter().enumerate() {
            if module_package(entry) == Some(name.as_str()) {
                push_module_rows(&mut out, diagram as u32, entry, key);
            }
        }
    }
    filter(out, search)
}

fn push_module_rows(out: &mut Vec<Row>, index: u32, diagram: &Diagram, parent: TreeKey) {
    let DiagramKind::Modules {
        target, kind, mode, ..
    } = &diagram.kind
    else {
        return;
    };
    out.push(Row {
        key: TreeKey::Diagram(index),
        parent: Some(parent),
        label: diagram_label(target, kind, *mode, &diagram.content),
    });
    if let Content::Drawn(scene) = &diagram.content {
        let parents = item_parents(index, scene);
        for (item, (entry, owner)) in scene.items.iter().zip(parents).enumerate() {
            out.push(Row {
                key: TreeKey::Item {
                    diagram: index,
                    item: item as u32,
                },
                parent: Some(owner),
                label: entry.name.clone(),
            });
        }
    }
}

fn diagram_label(target: &str, kind: &str, mode: Mode, content: &Content) -> String {
    let mode = match mode {
        Mode::Normal => "normal",
        Mode::Test => "test",
    };
    let state = match content {
        Content::Drawn(_) => "",
        Content::Failed(_) => " — failed",
        Content::Skipped(_) => " — skipped",
    };
    format!("{target} ({kind}, {mode}){state}")
}

/// 要素の親。その要素の region の親の region の要素。無ければ図そのもの。
fn item_parents(index: u32, scene: &Scene) -> Vec<TreeKey> {
    let mut parents = vec![TreeKey::Diagram(index); scene.items.len()];
    for region in &scene.regions {
        let parent = region.parent.and_then(|p| scene.regions.get(p as usize));
        if let (Some(parent), Some(slot)) = (parent, parents.get_mut(region.item as usize)) {
            *slot = TreeKey::Item {
                diagram: index,
                item: parent.item,
            };
        }
    }
    parents
}

/// package の名前を昇順に、重複なく並べる。crate 図が Drawn なら、その要素の名前も含む。
fn packages(loaded: &Loaded) -> Vec<String> {
    let mut names = BTreeSet::new();
    if let Some(scene) = crate_scene(loaded) {
        names.extend(scene.items.iter().map(|entry| entry.name.clone()));
    }
    names.extend(
        loaded
            .archive
            .diagrams
            .iter()
            .filter_map(module_package)
            .map(str::to_owned),
    );
    names.into_iter().collect()
}

fn crate_scene(loaded: &Loaded) -> Option<&Scene> {
    let index = *loaded.kinds.get(&DiagramKind::Crates)?;
    match &loaded.archive.diagrams.get(index)?.content {
        Content::Drawn(scene) => Some(scene),
        Content::Failed(_) | Content::Skipped(_) => None,
    }
}

fn module_package(diagram: &Diagram) -> Option<&str> {
    match &diagram.kind {
        DiagramKind::Modules { package, .. } => Some(package.as_str()),
        DiagramKind::Crates => None,
    }
}

fn filter(rows: Vec<Row>, search: &str) -> Vec<Row> {
    let needle = search.to_lowercase();
    if needle.is_empty() {
        return rows;
    }
    let parents: BTreeMap<TreeKey, Option<TreeKey>> =
        rows.iter().map(|row| (row.key, row.parent)).collect();
    let mut keep = BTreeSet::new();
    for row in rows
        .iter()
        .filter(|row| row.label.to_lowercase().contains(&needle))
    {
        let mut next = Some(row.key);
        while let Some(key) = next {
            if !keep.insert(key) {
                break;
            }
            next = parents.get(&key).copied().flatten();
        }
    }
    rows.into_iter()
        .filter(|row| keep.contains(&row.key))
        .collect()
}

/// 残した行の親の key（展開する枝）。
fn branch_keys(rows: &[Row]) -> Vec<TreeKey> {
    let keys: BTreeSet<TreeKey> = rows.iter().filter_map(|row| row.parent).collect();
    keys.into_iter().collect()
}

/// 木の選択の処理。package は、crate 図の要素の open があればそれ、無ければ最初の module 図。
pub fn action(loaded: &Loaded, key: TreeKey) -> Option<Action> {
    match key {
        TreeKey::Package(index) => package_action(loaded, index),
        TreeKey::Diagram(diagram) => Some(Action::Open(diagram as usize)),
        TreeKey::Item { diagram, item } => Some(Action::Reveal(diagram as usize, item)),
    }
}

fn package_action(loaded: &Loaded, index: u32) -> Option<Action> {
    let packages = packages(loaded);
    let name = packages.get(index as usize)?;
    let open = crate_scene(loaded)
        .and_then(|scene| scene.items.iter().find(|entry| &entry.name == name))
        .and_then(|entry| entry.open);
    if let Some(diagram) = open {
        return Some(Action::Open(diagram as usize));
    }
    loaded
        .archive
        .diagrams
        .iter()
        .position(|diagram| module_package(diagram) == Some(name.as_str()))
        .map(Action::Open)
}

#[cfg(test)]
mod tests {
    //! Role: サイドバーの行・検索・操作の試験。
    use super::*;
    use std::path::PathBuf;
    use xtask_archive::{
        Archive, Content, Diagram, DiagramKind, Item, Mode, Node, Rect, Region, Scene,
    };

    fn item(name: &str, open: Option<u32>) -> Item {
        Item {
            name: name.into(),
            role: None,
            open,
        }
    }

    fn region(item: u32, parent: Option<u32>) -> Region {
        Region {
            item,
            parent,
            rect: Rect::default(),
            lines: Vec::new(),
        }
    }

    fn scene(items: Vec<Item>, regions: Vec<Region>) -> Scene {
        let nodes = (0..items.len())
            .map(|index| Node {
                item: index as u32,
                rect: Rect::default(),
                lines: Vec::new(),
            })
            .collect();
        Scene {
            width: 100,
            height: 100,
            items,
            nodes,
            regions,
            edges: Vec::new(),
        }
    }

    fn modules(package: &str, content: Content) -> Diagram {
        Diagram {
            kind: DiagramKind::Modules {
                package: package.into(),
                target: "lib".into(),
                kind: "lib".into(),
                mode: Mode::Normal,
            },
            title: format!("{package} lib"),
            content,
        }
    }

    /// 手で作った文書。crate 図は `crate_content`、package a の module 図は index 1。
    /// module 図の `pictures` は、この試験が読まないので None にする。
    fn sample(crate_content: Content) -> Loaded {
        let module = Content::Drawn(scene(
            vec![item("crate", None), item("crate::x", None)],
            vec![region(0, None), region(1, Some(0))],
        ));
        let diagrams = vec![
            Diagram {
                kind: DiagramKind::Crates,
                title: "crates".into(),
                content: crate_content,
            },
            modules("a", module),
        ];
        let kinds = diagrams
            .iter()
            .enumerate()
            .map(|(index, d)| (d.kind.clone(), index))
            .collect();
        let pictures = diagrams.iter().map(|_| None).collect();
        let archive = Archive {
            root_name: "repo".into(),
            diagrams,
            notices: Vec::new(),
        };
        Loaded {
            path: PathBuf::from("sample.xtaskmap"),
            archive,
            pictures,
            kinds,
        }
    }

    fn drawn_crates() -> Content {
        Content::Drawn(scene(vec![item("a", Some(1)), item("b", None)], Vec::new()))
    }

    #[test]
    fn rows_follow_packages_diagrams_and_modules() {
        let out = rows(&sample(drawn_crates()), "");
        let keys: Vec<TreeKey> = out.iter().map(|row| row.key).collect();
        assert_eq!(
            keys,
            vec![
                TreeKey::Package(0),
                TreeKey::Diagram(1),
                TreeKey::Item {
                    diagram: 1,
                    item: 0
                },
                TreeKey::Item {
                    diagram: 1,
                    item: 1
                },
                TreeKey::Package(1),
            ]
        );
        assert_eq!(
            out[3].parent,
            Some(TreeKey::Item {
                diagram: 1,
                item: 0
            })
        );
    }

    #[test]
    fn failed_crate_diagram_still_lists_packages() {
        let out = rows(&sample(Content::Failed("boom".into())), "");
        assert!(
            out.iter()
                .any(|row| row.key == TreeKey::Package(0) && row.label == "a")
        );
    }

    #[test]
    fn search_keeps_matches_and_ancestors() {
        let out = rows(&sample(drawn_crates()), "x");
        let keys: Vec<TreeKey> = out.iter().map(|row| row.key).collect();
        assert_eq!(
            keys,
            vec![
                TreeKey::Package(0),
                TreeKey::Diagram(1),
                TreeKey::Item {
                    diagram: 1,
                    item: 0
                },
                TreeKey::Item {
                    diagram: 1,
                    item: 1
                },
            ]
        );
    }

    #[test]
    fn actions_open_and_reveal() {
        let loaded = sample(drawn_crates());
        assert_eq!(action(&loaded, TreeKey::Package(0)), Some(Action::Open(1)));
        assert_eq!(action(&loaded, TreeKey::Diagram(1)), Some(Action::Open(1)));
        assert_eq!(
            action(
                &loaded,
                TreeKey::Item {
                    diagram: 1,
                    item: 1
                }
            ),
            Some(Action::Reveal(1, 1))
        );
        assert_eq!(action(&loaded, TreeKey::Package(1)), None);
    }
}

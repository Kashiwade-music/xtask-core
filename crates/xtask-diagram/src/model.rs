//! Role: 図の組立ての入力・計画・下書きの型と、それらの小さな helper を定義する。
use std::collections::BTreeMap;

use xtask_archive::{DiagramKind, EdgeStyle, Item, Mode};
use xtask_modules::ModuleGraph;
use xtask_workspace::{Package, Target, TargetKind, Workspace};

/// 一つの target・構成の解析の結果。
pub enum Outcome<'a> {
    Analyzed(&'a ModuleGraph),
    Failed(String),
    Skipped(String),
}

pub struct TargetInput<'a> {
    pub package: &'a str,
    pub target: &'a str,
    pub kinds: &'a [TargetKind],
    pub mode: Mode,
    pub outcome: Outcome<'a>,
    /// この target・構成の module 図を作るか（policy の `[diagrams] targets` で決まる）。
    pub drawn: bool,
}

pub struct Input<'a> {
    pub workspace: &'a Workspace,
    /// 解析したすべての target・構成（図を作らない target も含む。crate の Role を取るため）。
    pub targets: Vec<TargetInput<'a>>,
}

/// 配置の前の図。並列に `draw` できる。
#[derive(Debug)]
pub struct Plan {
    pub(crate) kind: DiagramKind,
    pub(crate) title: String,
    pub(crate) body: Body,
}

#[derive(Debug)]
pub(crate) enum Body {
    Graph(Drawing),
    Failed(String),
    Skipped(String),
}

/// 一つの関数（crate 図、module 図のそれぞれ）だけが作る。各 Vec は graph の同じ名前の配列と同じ長さ・同じ順。
#[derive(Debug)]
pub(crate) struct Drawing {
    pub(crate) graph: xtask_layout::Graph,
    pub(crate) items: Vec<Item>,
    /// node ごとの (item, lines)。
    pub(crate) nodes: Vec<(u32, Vec<String>)>,
    /// region ごとの (item, lines)。parent は graph から取る。
    pub(crate) regions: Vec<(u32, Vec<String>)>,
    /// edge ごとの (style, ラベルの lines)。
    pub(crate) edges: Vec<(EdgeStyle, Option<Vec<String>>)>,
}

/// (package, target, kind の文字列) から、その Normal の module 図の index への表。
pub(crate) type OpenIndices = BTreeMap<TargetKey, u32>;
pub(crate) type TargetKey = (String, String, String);

/// package の主な target（library を優先し、無ければ bin）。
pub fn primary_target(package: &Package) -> Option<&Target> {
    let library = package
        .targets
        .iter()
        .filter(|target| target.kinds.iter().any(TargetKind::is_library))
        .min_by(|left, right| (&left.name, &left.src_path).cmp(&(&right.name, &right.src_path)));
    library.or_else(|| {
        package
            .targets
            .iter()
            .filter(|target| target.kinds.contains(&TargetKind::Bin))
            .min_by(|left, right| {
                (left.name != package.name, &left.name, &left.src_path).cmp(&(
                    right.name != package.name,
                    &right.name,
                    &right.src_path,
                ))
            })
    })
}

/// target の種類を `+` でつないだ文字列（表示と図の kind に使う）。
pub(crate) fn kinds_text(kinds: &[TargetKind]) -> String {
    kinds
        .iter()
        .map(TargetKind::as_str)
        .collect::<Vec<_>>()
        .join("+")
}

pub(crate) fn key(package: &str, target: &str, kinds: &[TargetKind]) -> TargetKey {
    (package.into(), target.into(), kinds_text(kinds))
}

pub(crate) fn module_kind(target: &TargetInput<'_>) -> DiagramKind {
    DiagramKind::Modules {
        package: target.package.into(),
        target: target.target.into(),
        kind: kinds_text(target.kinds),
        mode: target.mode,
    }
}

/// Normal の module 図の index（plan の順は Archive.diagrams の順で、crate 図の次から始まる）。
pub(crate) fn open_indices(drawn: &[(DiagramKind, &TargetInput<'_>)]) -> OpenIndices {
    drawn
        .iter()
        .enumerate()
        .filter(|(_, (_, target))| target.mode == Mode::Normal)
        .map(|(position, (_, target))| {
            (
                key(target.package, target.target, target.kinds),
                (position + 1) as u32,
            )
        })
        .collect()
}

/// 解析の成功した Normal の target の root module の Role。無ければ None。
pub(crate) fn analyzed_root_role(
    input: &Input<'_>,
    package: &Package,
    target: &Target,
) -> Option<String> {
    let found = input.targets.iter().find(|candidate| {
        candidate.package == package.name
            && candidate.target == target.name
            && candidate.kinds == target.kinds.as_slice()
            && candidate.mode == Mode::Normal
    })?;
    match &found.outcome {
        Outcome::Analyzed(graph) => graph.modules.first()?.role.clone().ok(),
        Outcome::Failed(_) | Outcome::Skipped(_) => None,
    }
}

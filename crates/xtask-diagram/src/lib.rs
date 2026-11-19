//! Role: 解析結果から図の配置要求を作り、配置して図のファイルの型へ組み立てる。
//! `plan` lists the diagrams of one archive and their layout requests without laying them out;
//! `draw` lays out one planned diagram, so the caller may run `draw` on several plans in parallel.
mod crate_graph;
mod module_graph;
mod scene;
#[cfg(test)]
mod tests;
mod text;

use std::collections::BTreeMap;

use xtask_archive::{Content, Diagram, DiagramKind, EdgeStyle, Item, Mode};
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

/// 図を並べ、配置の要求を作る。結果の順は Archive.diagrams の順。配置はしない。
pub fn plan(input: &Input<'_>) -> Vec<Plan> {
    let mut drawn: Vec<(DiagramKind, &TargetInput<'_>)> = input
        .targets
        .iter()
        .filter(|target| target.drawn)
        .map(|target| (module_kind(target), target))
        .collect();
    drawn.sort_by(|left, right| left.0.cmp(&right.0));
    let opens = open_indices(&drawn);
    let mut plans = vec![crate_graph::plan(input, &opens)];
    plans.extend(
        drawn
            .iter()
            .map(|(kind, target)| module_plan(kind.clone(), target)),
    );
    plans
}

/// 一つの図を配置する。配置に失敗したら Content::Failed にする。
pub fn draw(plan: &Plan) -> Diagram {
    let content = match &plan.body {
        Body::Graph(drawing) => match scene::draw_graph(drawing) {
            Ok(scene) => Content::Drawn(scene),
            Err(message) => Content::Failed(message),
        },
        Body::Failed(message) => Content::Failed(message.clone()),
        Body::Skipped(reason) => Content::Skipped(reason.clone()),
    };
    Diagram {
        kind: plan.kind.clone(),
        title: plan.title.clone(),
        content,
    }
}

/// 配置の worker が panic したときの図（kind と title は plan のもの）。
pub fn failed(plan: &Plan, message: String) -> Diagram {
    Diagram {
        kind: plan.kind.clone(),
        title: plan.title.clone(),
        content: Content::Failed(message),
    }
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

fn module_kind(target: &TargetInput<'_>) -> DiagramKind {
    DiagramKind::Modules {
        package: target.package.into(),
        target: target.target.into(),
        kind: kinds_text(target.kinds),
        mode: target.mode,
    }
}

fn module_plan(kind: DiagramKind, target: &TargetInput<'_>) -> Plan {
    let mode = match target.mode {
        Mode::Normal => "normal",
        Mode::Test => "test",
    };
    let title = format!(
        "{}::{} ({}, {mode}): module dependencies",
        target.package,
        target.target,
        kinds_text(target.kinds)
    );
    let body = match &target.outcome {
        Outcome::Analyzed(graph) => match module_graph::drawing(graph) {
            Ok(drawing) => Body::Graph(drawing),
            Err(message) => Body::Failed(message),
        },
        Outcome::Failed(message) => Body::Failed(message.clone()),
        Outcome::Skipped(reason) => Body::Skipped(reason.clone()),
    };
    Plan { kind, title, body }
}

/// Normal の module 図の index（plan の順は Archive.diagrams の順で、crate 図の次から始まる）。
fn open_indices(drawn: &[(DiagramKind, &TargetInput<'_>)]) -> OpenIndices {
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

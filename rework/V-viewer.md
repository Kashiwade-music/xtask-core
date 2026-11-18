# 付属書 V：viewer（`viewer/`）と CI

設計書 5.5 節の契約を実装する手順。viewer は、このリポジトリの中の**別の Cargo workspace** として置く。Kivact を使うため、Rust のツールチェーンは 1.98.1 にする。

## 0. Kivact の事実（固定する commit で確かめたもの）

- 依存は `kivact = { git = "https://github.com/Kashiwade-music/kivact", rev = "2b8df5aceea40c9ae23950f9d6c942559c89e840" }`。
  - この commit は Kivact の `main`。改修中の branch の API は使わない。
  - 試験では `features = ["test"]` を足す。
- **文書。** 固定した commit の文書を読むときは、`git -C W:/w/kivact show 2b8df5ac:<path>` を使う。手元の checkout は別の branch にあるので、作業ツリーのファイルを直接読まない。
  - 入門：`docs/for-kivact-user/en/getting-started/`
  - 描画：`docs/for-kivact-user/en/how-to-guide/canvas.md`
  - 配置：`how-to-guide/layout.md`
  - 非同期：`how-to-guide/async-and-tasks.md`
  - ファイルのダイアログ：`how-to-guide/platform-services.md`
- **よく使う API**（すべて `kivact::prelude::*` か、明記した path にある）：

| 用途 | API |
|---|---|
| 起動 | `App::new(title).window_size(1280, 800).run(|| root())` |
| 状態 | `signal(v) -> Signal<T>`（`get`、`get_untracked`、`with`、`set`、`update`）、`memo(f) -> Value<T>`、`effect(f)` |
| 非同期 | `spawn(async move { .. })`、`blocking(move || ..).await`、`kivact::task::sleep(Duration).await` |
| 配置 | `row((..))`、`column((..))`、`scroll(child)`、`style().width(280).grow(1).min_width(0).height(pct(100)).gap(8).padding(8)` |
| 部品 | `button(label).on_press(..)`、`text_input(label, signal).placeholder(..)`、`select(label, items, selected)`、`text(..)` |
| 木 | `kivact::ui::tree(label, items: Vec<TreeNode<K>>, selected: Signal<Option<K>>)` に `.expanded(signal(vec))`、`.on_activate(|k| ..)` を付ける。node は `kivact::interaction::TreeNode::new(key, parent: Option<K>, label)` |
| 描画 | `canvas(move |draw, size| { .. })`。`draw.layer(Layer::new().transform(Transform::new().translate(x, y).scale(s)), |d| ..)`、`draw.fill_path`、`draw.stroke_path(&path, &stroke, paint)`、`draw.text(str, Point, TextFormat)`、`draw.region(RegionId(n), rect)` |
| path | `kivact::draw::Path::new().move_to(p).line_to(p).quad_to(c, p).svg_arc_to((r, r), 0.0, false, sweep, p).close()` |
| 線 | `Stroke::new(1.3)`。点線は `Stroke { cap: LineCap::Round, dash: vec![0.0, 3.5], ..Stroke::new(1.3) }` |
| 文字 | `TextFormat { size, family: Some(FONT_FAMILY.into()), align: TextAlign::Center, baseline: TextBaseline::Alphabetic, color, ..TextFormat::default() }` |
| region | 宣言は `canvas.regions(items, key, make)`。make は `kivact::custom::Part::new(().into_value(), move |host| host.role(Role::Button).name(name).focusable(true).on_key(..))`。描画の中で `draw.region(id, rect)` を呼ぶ。canvas に `.region_events()` を付けると、region の入力が canvas の handler にも届く |
| pointer | `.on_pointer(|e: PointerEvent| ..)`。`e.phase()` は `PointerPhase::{Down(press), Move, Up(press), Cancel, Enter, Leave}`。`press.clicks >= 2` がダブルクリック。`e.pos()` は canvas 座標、`e.window_pos()` は window 座標、`e.region() -> Option<RegionId>` |
| wheel | `.on_wheel(|e: WheelEvent| ..)`。`e.delta().y` が正なら下へ回したとき、`e.pos()` |
| key | `.on_key(|e: KeyEvent| ..)`。`e.key()` は `Key::{Escape, Enter, Space, F(5), Char('0'), Backspace, ..}`。何も focus していないときは root の view に届く |
| ファイル | `kivact::platform::{pick_file, save_file, FileDialog}`（async、`Ok(None)` は取消し）。drag & drop は `.on_file(|e: kivact::view::FileEvent| ..)` で、`e.phase == FilePhase::Drop` のときの `e.files` |
| tooltip | `kivact::view::overlay_at(point_value, Placement::Below, child)` を `show_some` の中で使う |
| SVG | `kivact::draw::Offscreen::new().render_svg(w, h, |draw, size| ..) -> Result<String>`。region は描かない |
| 試験 | `kivact::test::render(|| view)`、`screen.get_by_role(Role::Button, name).click()`、`.double_click()`、`screen.press_key(Key::Escape)`、`screen.advance(Duration)`、`screen.run_until_idle()`、`get_by_text`、`query_by_text` |

- **注意。**
  - canvas の closure の中で signal を書かない。
  - UI の thread で `std::thread::sleep` とファイルの I/O をしない（`blocking` を使う）。
  - 試験は環境変数 `KIVACT_TEST_MIRROR=off` で実行する。複数の試験の process を同時に走らせない。

---

## V1：workspace の骨組み、`xtask-render`、CI

### 所有

- `viewer/Cargo.toml`、`viewer/rust-toolchain.toml`、`viewer/Cargo.lock`
- `viewer/crates/xtask-render/**`
- `viewer/crates/xtask-viewer/Cargo.toml`、`src/lib.rs`、`src/main.rs`、`src/state.rs`、V2 のファイルの骨組み（手順 6）
- `viewer/dev/xtask/policy/workspace.toml`
- `.github/workflows/viewer-windows.yml`

### 手順

1. **`viewer/Cargo.toml`。**

   ```toml
   [workspace]
   members = ["crates/xtask-render", "crates/xtask-viewer"]
   resolver = "3"

   [workspace.package]
   version = "0.1.0"
   edition = "2024"
   license = "MIT OR Apache-2.0"

   [workspace.dependencies]
   kivact = { git = "https://github.com/Kashiwade-music/kivact", rev = "2b8df5aceea40c9ae23950f9d6c942559c89e840" }
   xtask-archive = { path = "../crates/xtask-archive" }
   xtask-render = { path = "crates/xtask-render" }
   ```

   この後に、Kivact の `templates/basic/README.md`（`git -C W:/w/kivact show 2b8df5ac:templates/basic/README.md`）にある `[profile.dev.package.*]` の block と、任意の release の profile の block を、そのまま写す。
2. **`viewer/rust-toolchain.toml`。**

   ```toml
   [toolchain]
   channel = "1.98.1"
   profile = "minimal"
   components = ["clippy", "rustfmt"]
   ```

3. **`viewer/crates/xtask-render`。** 依存は `xtask-archive` だけ。Kivact に依存しない純粋な crate にする。図の `Scene` から、描くものの一覧（`Picture`）を作る。

   ```rust
   //! Role: 図の Scene から、角の丸め・交差の跳び・矢印・文字の位置を決めた描画の一覧を作る。
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct P { pub x: f64, pub y: f64 }
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct R { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct Rgb(pub u8, pub u8, pub u8);
   #[derive(Clone, Debug, PartialEq)] pub enum Cmd { Move(P), Line(P), Quad { control: P, to: P }, Arc { radius: f64, sweep: bool, to: P } }
   #[derive(Clone, Debug, PartialEq)] pub struct Text { pub center_x: f64, pub baseline: f64, pub size: f64, pub text: String }
   #[derive(Clone, Debug, PartialEq)] pub struct NodeShape { pub item: u32, pub rect: R, pub texts: Vec<Text> }
   #[derive(Clone, Debug, PartialEq)] pub struct RegionShape { pub item: u32, pub rect: R, pub fill: Option<Rgb>, pub band: R, pub band_fill: Rgb, pub texts: Vec<Text> }
   #[derive(Clone, Debug, PartialEq)] pub struct EdgeShape { pub from_item: u32, pub to_item: u32, pub dotted: bool, pub path: Vec<Cmd>, pub arrow: [P; 3], pub label: Option<(R, Vec<Text>)> }
   #[derive(Clone, Debug, PartialEq)] pub struct Picture { pub width: f64, pub height: f64, pub title: Text, pub regions: Vec<RegionShape>, pub edges: Vec<EdgeShape>, pub nodes: Vec<NodeShape> }
   pub fn picture(title: &str, scene: &xtask_archive::Scene) -> Picture;
   pub fn neighbors(scene: &xtask_archive::Scene, item: u32) -> Highlight;
   #[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct Highlight { pub items: std::collections::BTreeSet<u32>, pub edges: std::collections::BTreeSet<u32> }
   pub mod palette { /* 3.4 の定数 */ }
   ```

   `picture` の規則は、現在の `crates/xtask-svg/src/render.rs`（C2 で消す前に読む）を写す。

   - **余白と題。**
     - 全体の大きさは `width = max(scene.width, 題の幅) + 32`、`height = scene.height + 58`。題の幅は `xtask_archive::style::advance` の和。
     - 図の中身はすべて (16, 42) だけずらす。
     - 題は `center_x = width / 2`、`baseline = 21 + TITLE_FONT_SIZE * 0.35`。
   - **region。** 親から子の順に並べる。
     - 深さは最上位を 0 とする。最上位の `fill` は None、それ以外は `region_background`（深さ d の明るさ `0.8^d`）。
     - `band`（見出しの帯）は `(x + 3, y + 3, width - 6, 行数 * REGION_LINE_HEIGHT + 4)` で、`band_fill` は同じ色。最上位の帯の色は白。
     - 文字は帯の中央にそろえる。
   - **node。** 文字の行は、中央 `(x + w/2, y + h/2)` を基準に `render.rs` の `text` と同じ式で baseline を決める。
   - **edge。**
     - 点列はすでに直交している。各角を半径 `min(4, 前の辺の長さ/2, 次の辺の長さ/2)` の `Quad`（制御点は角）で丸める。
     - **跳び（交差）。** 水平の線分が、別の edge の垂直の線分と内部で交わる点ごとに、半径 4 の半円の `Arc` を水平の線分の上に入れる。
       - 交点が水平の線分の両端から 8 未満なら入れない（角の丸めと重なるため）。
       - 跳びは index の小さい edge にも大きい edge にも同じ規則で入れる。水平の線分だけが跳ぶ。
       - 同じ線分の上の跳びは、x の進む向きに並べる。
     - **矢印。** 最後の点を先端に、最後の線分の向き（必ず右向き）に、長さ `min(8, 最後の線分の長さ)`、半幅 `長さ * 0.4` の三角形を作る。
     - `dotted` は `EdgeStyle::Dev` のとき true。
     - ラベルは、`label.rect` を (−2, −1) 広げた白い矩形と、中央にそろえた文字。
   - `neighbors(item)`：
     - items には item 自身を入れる。
     - edges には、端の node の item が item である edge を入れる。
     - items には、それらの edge のもう一方の端の node の item も入れる。

4. **palette（`xtask-render` の `pub mod palette`）。**

   | 名前 | 値 |
   |---|---|
   | `STROKE` | `Rgb(0x47, 0x55, 0x69)` |
   | `NODE_STROKE` | `Rgb(0x33, 0x41, 0x55)` |
   | `REGION_STROKE` | `Rgb(0x94, 0xa3, 0xb8)` |
   | `TEXT` | `Rgb(0x0f, 0x17, 0x2a)` |
   | `ACCENT` | `Rgb(0x25, 0x63, 0xeb)` |
   | `BACKGROUND` | `Rgb(0xff, 0xff, 0xff)` |
   | `NODE_FILL` | `Rgb(0xff, 0xff, 0xff)` |

   線の太さは `EDGE_WIDTH = 1.3`、`NODE_STROKE_WIDTH = 1.2`、`REGION_STROKE_WIDTH = 1.0`、`ACCENT_WIDTH = 2.6`。node の角の半径は 2、region の角の半径は 4。

5. **`xtask-render` の試験。**
   - `corner_is_rounded`：L 字の edge の path が `Move, Line, Quad, Line` になる。
   - `crossing_adds_arc`：十字に交わる 2 本の edge で、水平の線の path に `Arc` が一つある。
   - `arrow_points_right`：矢印の先端が最後の点で、残りの二点の x が先端より小さい。
   - `neighbors_include_both_ends`：a→b の edge で、`neighbors(a)` の items が {a, b}、edges が {0} になる。
   - `title_widens_picture`：幅の小さい scene と長い題で、`width` が題の幅 + 32 以上になる。

6. **`xtask-viewer` の骨組み。**
   - `Cargo.toml`：
     - 依存は `kivact.workspace = true`、`xtask-archive.workspace = true`、`xtask-render.workspace = true`。
     - `[dev-dependencies]` に `kivact = { workspace = true, features = ["test"] }` を足す。
     - binary の名前は `xtask-viewer`。
   - crate は library と binary の両方を持つ。試験（V3）が library の module を呼ぶため。
     - `src/lib.rs` には `//! Role:` の行と、すべての module の `pub mod` を書く（`app`、`canvas`、`document`、`export`、`navigate`、`paint`、`sidebar`、`state`）。
     - `src/main.rs` には、`//! Role:` の行と次の一つだけを書く。

       ```rust
       fn main() -> kivact::Result<()> {
           kivact::prelude::App::new("xtask viewer")
               .window_size(1280, 800)
               .run(|| xtask_viewer::app::root(std::env::args().nth(1).map(std::path::PathBuf::from)))
       }
       ```

   - V2 の表の関数は、すべて `pub fn` にする。
   - `app::root` は V2 が書く。V1 では `todo!()` の本体で置く。
   - V2 の 3 lane が `lib.rs` を触らないよう、V1 は V2 の表のすべてのファイル（`app.rs`、`document.rs`、`export.rs`、`canvas.rs`、`paint.rs`、`navigate.rs`、`sidebar.rs`）を作る。
     - 中身は、`//! Role:` の行、表の公開の関数の署名と `todo!("<lane>")` の本体、`paint::Mode` の enum だけにする。
     - `todo!()` の骨組みは `dead_code`・`unused_variables` の警告を出す。V2 の lane が本体を書くまで、この警告は残してよい（作業指示書 0.6 節）。
7. **`src/state.rs`（このとおりに書く。V2 の 3 lane が共有する）。**

   ```rust
   //! Role: viewer の画面の状態（文書・表示中の図・視点・注目）を明示の型で定義する。
   use std::path::PathBuf;
   use std::rc::Rc;
   use std::time::SystemTime;
   use kivact::prelude::*;

   /// 開いているファイルの状態。
   #[derive(Clone)]
   pub enum Document {
       Empty,
       Loaded(Rc<Loaded>),
       Failed { path: PathBuf, message: String },
   }

   impl PartialEq for Document {
       fn eq(&self, other: &Self) -> bool {
           match (self, other) {
               (Self::Empty, Self::Empty) => true,
               (Self::Loaded(a), Self::Loaded(b)) => Rc::ptr_eq(a, b),
               (Self::Failed { path: a, message: m }, Self::Failed { path: b, message: n }) => a == b && m == n,
               _ => false,
           }
       }
   }

   pub struct Loaded {
       pub path: PathBuf,
       pub stamp: Stamp,
       pub archive: xtask_archive::Archive,
       /// diagrams と同じ順。Drawn のものだけ Some。
       pub pictures: Vec<Option<xtask_render::Picture>>,
   }

   /// 変更の検出に使うファイルの印。
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub struct Stamp { pub modified: SystemTime, pub len: u64 }

   /// 視点。Fit は canvas の大きさに合わせて描画のたびに求める。
   /// Manual では、画面の点 = 図の点 * scale + offset。
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub enum Camera {
       Fit,
       Manual { offset_x: f64, offset_y: f64, scale: f64 },
   }

   /// 注目の状態。Pinned の間は hover で変わらない。
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum Focus { Idle, Hover(u32), Pinned(u32) }

   /// 画面の状態の handle（Copy）。
   #[derive(Clone, Copy)]
   pub struct AppState {
       pub document: Signal<Document>,
       /// 表示中の図の index。
       pub diagram: Signal<usize>,
       /// 戻る先の図の index の列。
       pub back: Signal<Vec<usize>>,
       pub camera: Signal<Camera>,
       pub focus: Signal<Focus>,
       pub search: Signal<String>,
       /// 再読込みの失敗などの一時の通知。
       pub note: Signal<Option<String>>,
       /// 最後に描いた canvas の大きさ。描画の closure が書き、handler が読む（追跡しない）。
       pub viewport: Var<(f64, f64)>,
   }

   impl AppState {
       /// setup の中で呼ぶ。
       pub fn new() -> Self {
           Self {
               document: signal(Document::Empty),
               diagram: signal(0),
               back: signal(Vec::new()),
               camera: signal(Camera::Fit),
               focus: signal(Focus::Idle),
               search: signal(String::new()),
               note: signal(None),
               viewport: var((960.0, 640.0)),
           }
       }
   }
   ```

8. **`viewer/dev/xtask/policy/workspace.toml`。**

   ```toml
   [limits]
   max_code_lines = 1000

   [analysis]
   # kivact の prelude は型・trait・関数だけを与え、derive と attribute は与えない。
   trusted_globs = ["kivact::prelude"]

   [visible_to]
   xtask-render = ["xtask-viewer"]
   ```

9. **CI（`.github/workflows/viewer-windows.yml`）。** main に push があったときだけ build する。

   ```yaml
   name: viewer-windows
   on:
     push:
       branches: [main]
   permissions:
     contents: read
   jobs:
     build:
       runs-on: windows-latest
       steps:
         - uses: actions/checkout@v4
         - name: Install the viewer toolchain
           working-directory: viewer
           run: rustup toolchain install
         - uses: Swatinem/rust-cache@v2
           with:
             workspaces: viewer
         - name: Build
           working-directory: viewer
           run: cargo build --release --locked -p xtask-viewer
         - uses: actions/upload-artifact@v4
           with:
             name: xtask-viewer-windows
             path: viewer/target/release/xtask-viewer.exe
   ```

10. **lock。** `viewer` で `cargo generate-lockfile` を実行し、`viewer/Cargo.lock` を commit する。初回は network が要る。network が無ければ STOP にする。

### 確認

```bash
cd viewer && cargo check -p xtask-render --all-targets && cargo test -p xtask-render && cargo check -p xtask-viewer
```

最後の check は、`app::root` の `todo!()` の本体で通ればよい。

---

## V2：viewer の画面（3 lane）

3 lane は別々のファイルを所有する。共有するのは V1 の `state.rs` と、下の関数の署名だけ。署名はこのとおりに書く。

| lane | 所有するファイル | 公開する関数 |
|---|---|---|
| V2a | `src/app.rs`、`src/document.rs`、`src/export.rs` | `app::root(initial: Option<PathBuf>) -> impl IntoView`、`document::open(state: AppState, path: PathBuf)`、`document::watch(state: AppState)`、`export::export_svg(state: AppState)`、`pub(crate) fn load(path: &Path) -> Result<Loaded, String>`（document） |
| V2b | `src/canvas.rs`、`src/paint.rs`、`src/navigate.rs` | `canvas::diagram_view(state: AppState) -> impl IntoView`、`paint::picture(draw: &mut Draw, picture: &Picture, mode: Mode)`、`paint::highlight(draw: &mut Draw, picture: &Picture, highlight: &Highlight)`、`navigate::open(state: AppState, diagram: usize)`、`navigate::back(state: AppState)`、`navigate::fit(state: AppState)`、`navigate::reveal(state: AppState, diagram: usize, item: u32)`、`navigate::effective(camera: Camera, picture: &Picture, size: (f64, f64)) -> (f64, f64, f64)` |
| V2c | `src/sidebar.rs` | `sidebar::sidebar(state: AppState) -> impl IntoView` |

V1 が全ファイルの署名を置くので、各 lane は自分のファイルの `todo!()` を本体に替えるだけで compile が通る。他の lane の関数は `todo!()` のまま呼んでよい（実行はしない）。画面の試験は 3 lane の merge の後（V3）に置く。

### V2a：枠・文書・書出し

1. **`app::root`。**
   - `AppState::new()` を作る。`initial` が Some なら `document::open(state, path)` を呼ぶ。
   - `document::watch(state)` は、`initial` の有無にかかわらず一度だけ呼ぶ（後で開いたファイルも見張るため）。
   - toolbar の先頭に、今の図の題を `text(move || ..)` で出す。文書が Loaded でなければ、`No file open` か失敗の文を出す（V3 の試験が文で探す）。
   - 画面は `row((sidebar::sidebar(state), column((toolbar, canvas::diagram_view(state), notices))))` にする。
     - sidebar の幅は 300 にする。右の column は `grow(1).min_width(0)` にする。全体は `height(pct(100))` にする。
     - toolbar は、`Open…`（`pick_file`）、`Reload`、`Back`、`Fit`、`Export SVG` の button と、同じ package の図を切り替える `select` を並べる。
       - select の項目は、`archive.diagrams` のうち今の図と同じ package の Modules の図。label は `"{target} ({kind}, {normal|test})"`。
       - 選ぶと `navigate::open` を呼ぶ。
     - notices は、`archive.notices`、今の図が Failed・Skipped ならその文、`note` の文を、`column` に `text` で並べる。
   - root の `.on_file` は、Drop のとき最初のファイルで `document::open` を呼ぶ。
   - root の `.on_key` は次のとおり。どれも `Handled::Yes` を返し、それ以外は `Handled::No` を返す。

     | key | 動作 |
     |---|---|
     | `Key::Escape` | focus を Idle にする |
     | `Key::F(5)` | 今の path で `document::open` を呼ぶ |
     | `Key::Char('0')` | `navigate::fit` |
     | `Key::Backspace` | `navigate::back` |

2. **`document::load`。** UI の thread の外で呼ぶ前提の、純粋な関数にする。
   - `std::fs::metadata` で Stamp を作り、`std::fs::read` で読み、`xtask_archive::decode` で復号する。
   - Drawn の図ごとに `xtask_render::picture(&diagram.title, scene)` を作る。
   - 誤りは `"{path}: {error}"` の文にする。
3. **`document::open(state, path)`。** `spawn` の中で `blocking(move || load(&path)).await` を呼ぶ。
   - **成功したとき：**
     - 前の文書が Loaded なら、前の今の図の `DiagramKind` と同じ kind の図を新しい文書で探す。あれば `diagram` をその index にし、`camera` は保つ。無ければ `diagram = 0` にし、`back` を空にして `navigate::fit` を呼ぶ。
     - 前の文書が Loaded でなければ、`diagram = 0` にして fit する。
     - `focus = Idle`、`note = None`、`document = Loaded(Rc::new(loaded))` にする。
   - **失敗したとき：**
     - 前が Loaded なら、文書は保ち、`note = Some(文)` にする。
     - そうでなければ `document = Failed { path, message }` にする。
   - `blocking` の結果（`Loaded`）は `Send` である必要がある。`Loaded` は `Rc` を持たない（上の型のとおり）ので満たす。
4. **`document::watch(state)`。** `spawn` の中で次を繰り返す。
   - `kivact::task::sleep(Duration::from_millis(500)).await` で待つ。
   - `document` が Loaded なら、その path の Stamp を `blocking` で取る。取れて、かつ Loaded の Stamp と違えば、`document::open(state, path)` を呼ぶ。
   - 取れなければ、何もしない（書換えの途中の可能性）。
5. **`export::export_svg(state)`。** 今の図の Picture が無ければ何もしない。
   - `save_file(FileDialog { title: "Export SVG".into(), file_name: Some(format!("{}.svg", 題を英数字と '-' 以外を '_' にしたもの)), filters: vec![("SVG".into(), vec!["svg".into()])], ..Default::default() })` を呼ぶ。
   - path が得られたら、`kivact::draw::Offscreen::new().render_svg(picture.width, picture.height, |draw, _| paint::picture(draw, &picture, paint::Mode::Export))` で SVG を作り、`blocking` で書く。
   - 失敗は `note` に入れる。

### V2b：図の canvas

1. **`paint::Mode`。** `pub enum Mode { Screen, Export }`。Export のときは `draw.region` を呼ばない。
2. **`paint::picture`。** 描く順は次のとおり。
   1. 背景（白、全体）。
   2. 題。
   3. region の矩形（`REGION_STROKE` の 1.0 の線、角の半径 4、fill）。
   4. edge の path（`STROKE`、`EDGE_WIDTH`、dotted は点線）と矢印（`STROKE` の fill）。
   5. node の矩形（白の fill と `NODE_STROKE` の 1.2 の線、角の半径 2）と文字。
   6. region の帯（band の fill）と見出しの文字。
   7. edge のラベル（白い矩形と文字）。

   文字は `TextFormat { size, family: Some(xtask_archive::style::FONT_FAMILY.into()), align: TextAlign::Center, baseline: TextBaseline::Alphabetic, color: palette::TEXT, ..Default::default() }` で、行ごとに `draw.text(行, Point::new(center_x, baseline), format)` を呼ぶ。

   Mode が Screen のときは、hit の範囲を次のとおり描く。
   - Picture に region が一つでもあれば、region ごとに `draw.region(RegionId(index as u64), rect)` を親から子の順に描く。
   - region が無ければ、node ごとに描く。
   - RegionId の値は、region または node の index。
3. **`paint::highlight`。** `Highlight` の edge の path を `ACCENT`・`ACCENT_WIDTH` で描き直し、node と region の矩形の輪郭を `ACCENT` の 2.0 の線で描く。
4. **`canvas::diagram_view`。** 二つの canvas を `stack((base, overlay))` で重ねる。overlay には `.hit_testable(false)` を付ける。
   - **base。**
     - 読むのは `document`、`diagram`、`camera` だけ。
     - 描画の closure の最初で `state.viewport.set((size.width, size.height))` を呼ぶ。`Var` は追跡しない値なので、描画の中で書いてよい。
       - Kivact がこれを誤りとして報告したら STOP する。
     - `navigate::effective(camera, picture, size) -> (offset_x, offset_y, scale)` で実際の視点を求める。
       - Fit なら、`scale = min(1.0, min(w / pw, h / ph) * 0.95)` とし、中央にそろえた offset にする。
       - Manual ならその値を使う。
     - `Layer::new().transform(Transform::new().translate(offset_x, offset_y).scale(scale))` の中で、`paint::picture(draw, picture, Mode::Screen)` を呼ぶ。
   - **overlay。** 読むのは `document`、`diagram`、`camera`、`focus` だけ。focus が Hover か Pinned のときだけ、同じ transform の中で `paint::highlight(draw, picture, &xtask_render::neighbors(scene, item))` を呼ぶ。
   - **region の宣言。** base に `.regions(entries, key, make)` で行う。
     - entries は `memo` で作る `Vec<(u64, u32, String)>`（RegionId の値、item、名前）で、今の図の region（無ければ node）ごとに一つ。
     - make の Part は `role(Role::Button).name(名前).focusable(true).selected(move || state.focus.get() == Focus::Pinned(item))` に、次の `on_key` を付ける。
       - `Key::Enter`：item の `open` が Some なら `navigate::open`。
       - `Key::Space`：Pinned(item) なら Idle、それ以外は Pinned(item)。
       - どちらも `Handled::Yes`、他は `Handled::No`。
     - base に `.region_events()` を付ける。
   - **pointer。** base の `.on_pointer`。drag の状態は `var(None::<(f64, f64, f64, f64)>)`（押した位置と、その時の offset）で持つ。

     | phase | 条件 | 動作 |
     |---|---|---|
     | `Down(p)` | `p.clicks >= 2` で region に当たる | item の `open` があれば `navigate::open` |
     | `Down(p)` | `p.clicks == 1` で region に当たる | Pinned(item) なら Idle、それ以外は Pinned(item) |
     | `Down(p)` | region に当たらない | drag の開始を記録する |
     | `Move` | drag 中 | `camera = Manual { offset = 押した時の実際の offset + (pos - 押した位置), scale = 押した時の実際の scale }`（実際の値は `navigate::effective` と `viewport` で求める） |
     | `Move` | drag 中でない | region に当たり focus が Pinned でなければ Hover(item)、当たらず focus が Hover なら Idle |
     | `Up` | drag 中で、動いた距離が 3 未満 | focus を Idle にする |
     | `Up` | — | drag を終える |
     | `Leave`・`Cancel` | — | focus が Hover なら Idle、drag を終える |

   - **wheel。**
     - 実際の `(offset, scale)` を `navigate::effective` と `viewport` で求める。
     - `f = (-e.delta().y * 0.0015).exp()`、`s' = clamp(scale * f, 0.05, 8.0)` とする。
     - pointer の位置 `a` を固定点にして `offset' = a - (a - offset) * (s' / scale)` を求め、`camera = Manual { offset', s' }` にする。
     - `Handled::Yes` を返す。
   - **tooltip。** focus が Hover で、item の role があれば、pointer の window 座標に `overlay_at(.., Placement::Below, text(format!("{name}\n{role}")))` を出す。window 座標は `var` ではなく `signal(Point)` で持つ（overlay が読むため）。
5. **`navigate`。**
   - `open(d)`：今の図を `back` に push し、`diagram = d`、`focus = Idle` にして `fit` を呼ぶ。
   - `back()`：pop して `diagram` に入れ、fit する。
   - `fit()`：`camera = Camera::Fit`。
   - `effective(camera, picture, size)`：上の base の規則のとおり。`pub fn` にする。
   - `reveal(d, item)`：
     - `d` を開く（今の図と違えば `open`）。`focus = Pinned(item)` にする。
     - scale は `effective` の今の値を保つ。
     - item の node（無ければ region）の矩形の中心が、`viewport` の中央に来るよう offset を決め、`camera = Manual` にする。

### V2c：sidebar

1. **木の key。** `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)] enum TreeKey { Package(u32), Diagram(u32), Item { diagram: u32, item: u32 } }`。
2. **木の node の作り方**（`memo` で、`document` と `search` に依存）。
   - package（crate 図の item の順）ごとに `Package(i)` を作る。label は package 名。
   - その下に、その package の Modules の図ごとに `Diagram(d)` を作る。label は `"{target} ({kind}, {normal|test})"`。
   - Drawn の図では、その下に module の item ごとに `Item { diagram: d, item }` を作る。parent は、その item の region の親の region の item。最上位は `Diagram(d)`。
3. **絞込み。** search を小文字にした文字列が空でなければ、次をすべて満たす node だけを残す。
   - label の小文字が search を含む node、またはその子孫に含む node があれば、その node を残す。
   - 残す node の祖先も残す。
   - 絞込み中は、残した node をすべて展開する（`expanded` に入れる）。
4. **画面。** `column((text_input("Search", state.search).placeholder("Filter crates and modules"), scroll(tree).style(style().grow(1).min_height(0))))` にする。
   - tree の `on_activate`：
     - `Package(i)`：crate 図の item i の `open` があれば `navigate::open`。
     - `Diagram(d)`：`navigate::open(d)`。
     - `Item { diagram, item }`：`navigate::reveal(diagram, item)`。

### 確認（各 lane）

```bash
cd viewer && cargo check -p xtask-viewer
```

---

## V3：viewer の試験（merge の後、1 lane）

### 所有

`viewer/crates/xtask-viewer/tests/viewer.rs`

### 手順

試験用の Archive を `xtask_archive` の型で手で作る。
- crate 図：item `a`（open = 1）、`b`。edge a→b。
- module 図：module `crate`、`crate::x`。edge `crate::x`→`crate`。

これを一時ファイルに `encode` して書き、`kivact::test::render(move || app::root(Some(path)))` で開く。試験は次のとおり。

- **`opens_file_from_argument`**：`run_until_idle` の後、題 `Workspace crate dependencies (consumer -> provider)` が見つかる。
- **`click_pins_and_escape_clears`**：`get_by_role(Role::Button, "a").click()` の後、その locator の `is_selected()` が true になる。`press_key(Key::Escape)` の後は false になる。
- **`double_click_opens_module_diagram`**：`get_by_role(Role::Button, "a").double_click()` の後、`crate::x` の button が見つかる。
- **`search_filters_tree`**：search に `x` を入れると、tree に `crate::x` が残り、`b` が消える。
- **`reload_after_change`**：ファイルを、題を変えた Archive で書き直す。`advance(Duration::from_millis(600))` と `run_until_idle()` の後、新しい内容が見える。
- **`export_svg_contains_title`**：`paint::picture` を `Offscreen::render_svg` に渡した文字列が `<svg` と題を含む。

### 確認

```bash
cd viewer && KIVACT_TEST_MIRROR=off cargo test -p xtask-viewer --test viewer
cd viewer && cargo clippy --workspace --all-targets -- -D warnings
```

clippy の指摘が V2 の lane のファイルにあれば、0.4 節の「STOP にせず直すもの」の範囲で直してよい（W4 では V2 の lane は終わっているので、所有の衝突は無い）。

# xtask-core 改修 作業指示書

版 1.0（2026-11-18）。設計の正本は [REWORK_DESIGN.md](REWORK_DESIGN.md)（以下「設計書」）、unit の手順は付属書 `rework/*.md`（以下「付属書」）。本書は**進め方だけ**を書く。設計の判断は設計書と付属書が持つ。

目標：設計書 1.2 節の AC-1〜AC-7 をすべて満たす。

---

## 0. 読み方と規則

### 0.1 誰が何をするか

| 役割 | 担当 | すること |
|---|---|---|
| Orchestrator | 本書を受け取った AI（親） | wave を順に進める。lane ごとに worktree を作り、unit ごとに subagent を起動する。結果を merge し、gate を実行する。subagent の `STOP:` に 0.5 節の表で答える。**設計の判断をしない** |
| Implementer | subagent（**同時に最大 6 体**） | 割り当てられた unit を、指定の worktree の中で、付属書の手順どおりに実装する。確認のコマンドを実行し、結果を報告する |

subagent の起動：

| 実行環境 | 起動の方法 | モデル |
|---|---|---|
| Claude Code | Agent tool を `subagent_type: "general-purpose"`、`model: "haiku"`、`effort: "high"` で呼ぶ | Haiku 5.5、effort high |
| その他 | その環境の subagent の起動の道具に、1.2 節の雛形をそのまま渡す | ユーザーの指定 |

- 同時に動かすのは最大 6 体（lane ごとに 1 体）。subagent に subagent を作らせない。
- 同じ wave の lane は、互いに独立している（所有するファイルが重ならない）。全 lane を同時に起動してよい。
  - 唯一の例外は W1 の root の `Cargo.toml` の `[workspace.dependencies]`。A1 と M1 がそれぞれ別の行を足し、1.3 節の規則で merge する。
- Implementer は push・merge・PR の作成をしない。`main` に触れない。
- Orchestrator は wave の merge の後にだけ `git push origin refactor/kashiwade/native-diagrams` する（force push は禁止）。PR は作らない。`main` に merge しない（人間が PR で行う）。

### 0.2 禁止事項（全 unit 共通）

1. 互換のためのコードを書かない。旧名の alias、`#[deprecated]`、wrapper、旧と新の二重の経路を作らない。旧名は消し、呼出し元を全部直す。
2. CI の設定・workflow を足さない（付属書 V の V1 が作る `viewer-windows.yml` を除く）。
3. 付属書が名前を挙げていない機能の試験を足さない。ただし、その unit の変更が壊しうる不変条件を確かめる最小の試験は、その unit が所有する試験のファイルに足してよい。期待値を緩めない（許容差を広げる、assert を消す、`#[ignore]` を付ける）。
4. `#[allow(..)]`・`#[expect(..)]` を、警告の件数を減らす目的で足さない。
5. 1 関数 80 code 行、1 ファイル 1000 code 行、引数 7 個の上限を超えない。超えそうなら意味のある helper に分ける。
6. 実装の途中で `cargo test --workspace`、clippy を起動する style の試験、基準の全件の試験（`tests/baseline.rs`）、viewer の GUI の試験を実行しない（CP で Orchestrator が実行する）。unit で実行してよいのは 0.6 節と、付属書の unit の「確認」のコマンドだけ。
7. 依存の crate を足さない（設計書か付属書が名前を挙げたものを除く）。
8. 出力の順に影響する場所で `HashMap`・`HashSet` と乱数を使わない。
9. **行番号を信じない。** 付属書と設計書 7 章の `file:line` は、基点 `e7eb600` のもの。関数名・型名・field 名・周りのコードで場所を探す。
10. 設計書の 5 章（公開の契約）と違うコードを書かない。付属書の手順が契約と食い違うなら STOP する。
11. 所有ファイル（付属書の unit の「所有」と、4 章の表）の外を変えない。ただし、署名を変えた結果の「compile を通すための機械的な追従」は、同じ crate の中なら行ってよい。他の crate なら STOP する。
12. 生成物（`target/`、`viewer/target/`、`architecture/`、`*.xtaskmap`）を commit しない。
13. 新しく作る `.rs` のファイルは、すべて 1 行目を `//! Role: <そのファイルの役割を 1 行、80 字以内>` にする（設計書 6.3 節。xtask 自身の検査が要求する）。付属書がコード片に Role の行を書いていれば、その文を使う。

### 0.3 unit の読み方

付属書の各 unit は、所有、目的、手順、試験、確認の欄を持つ。

- 手順は番号の順に行う。
- 「このとおりに書く」と書いたコード片は、一字一句そのまま書く（`use` の過不足は compile error に従って直してよい）。
- 「試験」の欄の試験を先に書き、失敗することを確かめてから実装する。
- 本書の 4 章の表の「追加の指示」は、付属書より優先する。

### 0.4 停止して報告（STOP）

次の場合は、その項目の作業を止め、他の項目を続け、最後の報告に `STOP:` で始まる行を書く。

- 付属書の手順が実際のコードと合わない（関数・型・field が無い、型が違う、名前で探しても見つからない）。
- 手順を行うと、所有ファイルの外を変える必要がある（0.2 の 11 の追従を除く）。
- 同じ compile error を 3 回直しても消えない。
- 付属書の試験の期待値が、手順どおりに実装しても満たせない。期待値は変えない。
- 付属書のアルゴリズムの説明で、手順が一つに決まらない（二通り以上に読める）。
- 設計書 5 章の契約と、付属書の手順が食い違う。
- Kivact の API が付属書 V の 0 章の表と違う（固定した commit の `git show` で確かめた上で）。

STOP にせず unit の中で直すもの（設計の判断ではない）：

- 関数・ファイル・引数の上限を超えた（意味のある helper に分ける。引数は struct にまとめる）。
- clippy の機械的な指摘（clippy の提案どおりに直す）。
- rustfmt、`use` の過不足。

書式：`STOP: <unit> <file>:<関数名> <何が合わないか 1 文> / <試したこと 1 文>`。

### 0.5 Orchestrator の既定の判断表

Orchestrator は設計の判断をしない。`STOP:` に次の表が当たればその判断をする。当たらなければ、その項目を**設計者への差戻し**として `rework/OPEN.md`（無ければ作る）の「差戻し」節に 1 行で書き（unit、場所、STOP の文、付属書の節）、他の作業を続ける。差戻しが 1 件でも残っていれば、CP で完了を宣言しない。

| 状況 | 判断 |
|---|---|
| 行番号がずれているが、同じ名前の関数・型・field が近くにある | その名前に対して手順を行うよう、同じ subagent に指示する（SendMessage） |
| 所有ファイルの外の 1〜3 ファイルを変える必要がある | 同じ wave の他の lane の所有でなければ許可する。他の lane の所有なら、その項目を次の wave に回す |
| compile error が、他の lane の未 merge の変更を前提にしている | その unit の確認の失敗を記録し、merge の後の tier b で確かめる。tier b でも失敗すれば、その lane の fix にする（1.3 節） |
| subagent が確認のコマンドを実行していない | 同じ subagent に実行させる |
| 試験が期待値に届かない（配置の試験を含む） | 差戻し。期待値を緩めない |
| KPI（設計書 2 章）が目標に届かない | 差戻し |
| network が無く、viewer の依存を取得できない | V の lane を止め、人間に報告する |

### 0.6 unit で実行してよいコマンド

```bash
cargo fmt -p <crate>                                   # 必ず最後に
cargo check -p <crate> --all-targets                   # 何度でも
cargo clippy -p <crate> --all-targets -- -D warnings   # 付属書の「確認」が挙げたときだけ
cargo test -p <crate> <filter>                         # 付属書の「確認」が挙げた範囲だけ
cd viewer && cargo check -p <crate>                    # viewer の lane
cd viewer && cargo test -p xtask-render                # V1 だけ
```

- すべての cargo のコマンドに、環境変数 `CARGO_TARGET_DIR=<1.1 節の target>` を付ける。
- **骨組みの警告。** 付属書が `todo!()` の骨組みと明記したファイル（付属書 L の 1.4 節、付属書 V の V1 の手順 6）に由来する `dead_code`・`unused_variables` の警告は、その本体を書く unit が終わるまで残してよい。
  - その間、該当する crate には `clippy -D warnings` を実行しない。付属書の「確認」の欄に clippy があるのは、crate が完成した unit だけ。
  - 引数の名前を `_` で始めて警告を消すことはしない（署名が変わるため）。
  - それ以外の警告は直す。
- `cargo test` の filter が 0 件の試験しか実行しなかったら、失敗として扱う（出力の `running 0 tests` を見る）。
- W1〜W4 の間、`xtask-core` は compile できない（設計書 9 章）。`cargo check --workspace` は実行しない。

---

## 1. 体制

### 1.1 branch・worktree・target

- 統合の branch：`refactor/kashiwade/native-diagrams`。W0 で作る。
- **lane の worktree は wave ごとに作る。** wave `<W>` の lane `<L>`：

  ```bash
  git worktree add ../xtask-core-wt/W<W>-<L> -b wt/W<W>-<L> refactor/kashiwade/native-diagrams
  ```

  その lane の unit は、すべて同じ worktree で一つずつ行う（unit ごとに commit）。
- **target。**
  - xtask の crate は lane ごとに `../xtask-core-wt/target-<L>`（L は 1〜6）を使う。
  - viewer の crate は、Kivact の build が大きいため、lane をまたいで `../xtask-core-wt/target-viewer` を共有する。
  - 同じ target を使う 2 つの lane の build は、cargo の lock で順になる。これは正常。

### 1.2 subagent への指示の雛形

Orchestrator は各 unit に次の文をそのまま渡し、`<..>` だけを埋める。

```text
あなたは xtask-core の改修の unit <unit ID> の実装者です。あなた自身は subagent を作らないでください。
作業場所は <worktree の絶対 path> です。この directory の外の file を変更しないでください。
すべての cargo のコマンドに次の環境変数を付けてください：CARGO_TARGET_DIR=<target の絶対 path>
最初に次を読んでください：
  1. <worktree>/REWORK_WORK_ORDER.md の 0 章（規則）と、4 章の表の「<unit ID>」の行（追加の指示を含む）
  2. <worktree>/rework/<付属書のファイル> の、<unit ID> の節と、その節が参照する節
  3. <worktree>/REWORK_DESIGN.md の 5 章と 6 章のうち、その節が参照する箇所
付属書の手順を番号の順に実行してください。「試験」の欄の試験を先に書いてください。行番号はずれています。関数名・型名・field 名で場所を探してください。
本書の「追加の指示」が付属書と食い違うときは本書に従ってください。設計書 5 章の契約と付属書が食い違うときは STOP してください。
設計の判断が必要になったら作業を止め、0.4 節の書式で STOP: 行を書いてください。
付属書の「確認」のコマンドをすべて実行し、error と警告を直してください。最後に cargo fmt -p <変更した各 crate> を実行してください。
最後に `git -C <worktree> add -A && git -C <worktree> commit -m "<unit ID>: <要約>"` で commit してください。push・merge はしないでください。
報告には次だけを書いてください：変更した file の一覧、確認の各コマンドの結果（成功/失敗と最後の数行）、試験の各項目の結果、STOP: 行。
```

### 1.3 merge の手順（Orchestrator、各 wave の終わり）

4 章の表の lane の順に、次を実行する。

```bash
git checkout refactor/kashiwade/native-diagrams
git merge --no-ff wt/W<W>-<L> -m "rework W<W>-<L>: <unit ID の列>"
```

- **conflict：**
  - root の `Cargo.toml` の `members`・`[workspace.dependencies]` の行、`dev/xtask/policy/workspace.toml` の行、`lib.rs` の `mod`・`pub use` の行は、両方の行を残し、アルファベット順に並べる。
  - `Cargo.lock` と `viewer/Cargo.lock` は、`git checkout --theirs <lock>` の後に `cargo metadata --format-version 1 --offline > /dev/null`（viewer では `cd viewer` してから）で作り直す。
  - それ以外は、4 章の表でそのファイルを所有する lane の版を採る。同じ wave で二つの lane が同じファイルを持たないように表を作ってある。それでも起きたら作業を止め、人間に報告する。
- 全 lane を merge したら、2 章の **tier b** を 1 回実行する。
  - 失敗したら、error を出した crate を所有した lane の最後の unit の subagent に、error の全文を渡して直させる。
  - 直しは、統合の branch の今の HEAD から作った worktree `../xtask-core-wt/W<W>-<L>-fix`（branch `wt/W<W>-<L>-fix`）で行い、同じ手順で merge する。
- merge の後、worktree と branch を消す：`git worktree remove ../xtask-core-wt/W<W>-<L> && git branch -D wt/W<W>-<L>`。
- push する。
- `rework/PROGRESS.md`（無ければ作る）に 1 行を足す：`W<W> merged <commit>: <unit の列>、tier b 成功`。

---

## 2. gate の 3 段

### 2.1 tier a（unit ごと、Implementer）

付属書の各 unit の「確認」のコマンド。

### 2.2 tier b（wave の merge の後、Orchestrator）

| wave | コマンド |
|---|---|
| W1 | `cargo check -p xtask-layout -p xtask-archive -p xtask-policy -p xtask-workspace -p xtask-source --all-targets`、`cargo test -p xtask-archive -p xtask-policy -p xtask-workspace -p xtask-source`、`cargo test -p xtask-layout --lib` |
| W2 | W1 のもの、`cargo check -p xtask-modules --all-targets`、`cargo test -p xtask-modules`、`cargo test -p xtask-layout --lib layering:: chains:: order:: position:: route::`（filter ごとに別のコマンドにする）、`cd viewer && cargo check --workspace && cargo test -p xtask-render` |
| W3 | W2 のもの、`cargo test -p xtask-layout --lib`（全単体試験。L6 の `verify_accepts_simple_layout` はここで初めて通る）、`cargo check -p xtask-style --all-targets`、`cargo test -p xtask-style --lib`、`cd viewer && cargo check --workspace` |
| W4 | W3 のもの、`cargo check -p xtask-diagram --all-targets`、`cargo test -p xtask-diagram`、`cd viewer && KIVACT_TEST_MIRROR=off cargo test -p xtask-viewer --test viewer` |
| W5 | `cargo check --workspace --all-targets`、`cargo test -p xtask-core --lib`、`cargo test -p xtask-core --test integration` |

### 2.3 tier c（CP、Orchestrator）

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                              # xtask-style の tests/tools.rs（clippy を起動する試験）を含む
cargo test -p xtask-layout --release --test baseline
cargo run -p xtask-layout --release --example baseline_report       # K1〜K4 を記録
cargo run -p xtask-core -- check                                    # 違反 0 件
cargo run -p xtask-core -- check --write                            # K5 を記録。PATH から node を外して実行する（AC-1）
cargo run -p xtask-core -- check --root viewer                      # viewer 自身の検査
cd viewer && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && KIVACT_TEST_MIRROR=off cargo test --workspace
git ls-files | grep -E '\.(js|cjs|html|css)$'                      # 何も出ないこと（AC-1。C2 が基準の取得の script も消す）
```

- `node` を外す方法（Git Bash）：`PATH=$(echo "$PATH" | tr ':' '\n' | grep -vi nodejs | paste -sd:) cargo run -p xtask-core -- check --write`。
- 結果と KPI の値を `rework/PROGRESS.md` に書く。
- 設計書 7 章の「対応」の所見を一件ずつ点検し、閉じていないものを差戻しにする（AC-5）。

---

## 3. W0：準備（Orchestrator が自分で行う）

1. 設計書・作業指示書・付属書が commit 済みであることを確かめる。
2. `git checkout -b refactor/kashiwade/native-diagrams` で統合の branch を作る（今の HEAD から）。
3. `git push -u origin refactor/kashiwade/native-diagrams`。
4. `../xtask-core-wt/` を作る。
5. 改修前の K6 の基準を測って `rework/PROGRESS.md` に書く：`cargo run -p xtask-core -- check --no-write` の 2 回目の時間。

---

## 4. wave の表

所有のファイルは付属書の各 unit の「所有」の欄。下の表の「追加の指示」は付属書より優先する。

### W1

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L1 | L 1.2〜1.4 節、2 章、6 章 | `xtask-layout` の旧いファイル（1.2 節の「消すもの」）を消す。1.4 節の骨組み（全ファイルの署名と `lib.rs` の完成形）を作る。後の unit は `lib.rs` を変えない |
| 2 | A1 | A 1 章 | root の `Cargo.toml` の `[workspace.dependencies]` に 2 行を足すのは、この unit |
| 3 | M1 | M の M1 | `xtask_source::normalize` は M2 が作る。compile が通らなければ報告に書く（STOP にしない） |
| 4 | M2 | M の M2 | — |

### W2

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L2 | L 3 章、4 章 | `layering.rs` と `chains.rs` の両方 |
| 2 | L3 | L 5 章 | 試験の Chains は手で作る |
| 3 | L4 | L 7 章 | 試験の Chains と Order は手で作る |
| 4 | L5 | L 8 章 | 試験の入力は手で作る |
| 5 | M3 | M の M3 | `dev/xtask/policy/workspace.toml` は `trusted_macros` の行だけを変える |
| 6 | V1 | V の V1 | network が要る。target は `target-viewer` |

### W3

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L6 | L 10 章 | `verify_accepts_simple_layout` は L7 の merge まで通らない。`#[ignore]` は付けず、報告に書く。Orchestrator が W3 の tier b の後に確かめる |
| 2 | L7 | L 9 章 | `hierarchy.rs` と `level.rs` の `run_level` の本体。`lib.rs` は変えない |
| 3 | C1 | C の C1 | `xtask-core` からファイルを移すだけで、xtask-core 側の compile は直さない（W5 の C2 が直す） |
| 4 | V2a | V の V2 の V2a | target は `target-viewer` |
| 5 | V2b | V の V2 の V2b | target は `target-viewer` |
| 6 | V2c | V の V2 の V2c | target は `target-viewer` |

### W4

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | A2 | A 2 章 | — |
| 2 | L8 | L 11 章 | `tests/baseline.rs` は compile だけ確かめる（`cargo test -p xtask-layout --release --test baseline --no-run`）。実行は CP |
| 3 | V3 | V の V3 | 試験は一回だけ実行する |
| 4 | C3 | C の C3 | 設計書 3 章と 5.1 節に従う。実装の結果ではなく設計書を正とする |

### W5

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | C2 | C の C2 | 一番大きい unit。手順 1（削除）の後に一度 commit してよい。確認の統合の試験は、付属書が挙げた 3 件だけを実行する |

### CP（W5 の後）

2.3 節の tier c。差戻しが 0 件で、AC-1〜AC-7 を満たせば完了。`rework/PROGRESS.md` に `CP passed <commit>` を書き、人間に PR の作成を依頼する。

---

## 5. unit の索引

| unit | wave | lane | 付属書 | 前提 |
|---|---|---|---|---|
| L1 | W1 | 1 | L | — |
| A1 | W1 | 2 | A | — |
| M1 | W1 | 3 | M | — |
| M2 | W1 | 4 | M | — |
| L2 | W2 | 1 | L | L1 |
| L3 | W2 | 2 | L | L1 |
| L4 | W2 | 3 | L | L1 |
| L5 | W2 | 4 | L | L1 |
| M3 | W2 | 5 | M | M2 |
| V1 | W2 | 6 | V | A1 |
| L6 | W3 | 1 | L | L1 |
| L7 | W3 | 2 | L | L2〜L5 |
| C1 | W3 | 3 | C | M1、M2 |
| V2a | W3 | 4 | V | V1 |
| V2b | W3 | 5 | V | V1 |
| V2c | W3 | 6 | V | V1 |
| A2 | W4 | 1 | A | A1、L7、M1、M3 |
| L8 | W4 | 2 | L | L6、L7 |
| V3 | W4 | 3 | V | V2a〜V2c |
| C3 | W4 | 4 | C | — |
| C2 | W5 | 1 | C | すべて |

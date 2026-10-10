# xtask-core

## これはなに

Rust プロジェクトの xtask として使える共通ルール。

下記のようにして利用する。

```plaintext
各リポジトリ/
├─ vendor/xtask-core/             ← 共通リポジトリの submodule
└─ dev/xtask/
   ├─ Cargo.toml                 ← xtask-core への path 依存
   ├─ src/main.rs                ← リポジトリ固有の設定を渡すだけ
   └─ policy/workspace.toml      ← そのリポジトリの規則
```

## ルール一覧

- コメント・文字列を除外する1ファイルあたりのコード行数が1000行以下
- crate単位の依存解析とSVG書き出し
- `visible_to`: 当該crateに依存できるcrateの許可リスト検査
- 各crate内モジュールの階層構造と参照構造のSVG書き出し
- 各crate内モジュールの参照構造の循環禁止

## SVG書き出しについて

dotで作成し、Graphviz経由でSVGに変換する。

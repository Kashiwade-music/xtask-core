//! Role: 単一HTMLへの図の埋め込み、選択・失敗・非書込み時の整合性を検証する。
use super::*;
use serde_json::Value;

fn payload(html: &str) -> (&str, Value) {
    let marker = "<script id=\"architecture-data\" type=\"application/json\">";
    let json = html
        .split_once(marker)
        .expect("embedded architecture data")
        .1
        .split_once("</script>")
        .expect("closed architecture data")
        .0;
    (json, serde_json::from_str(json).unwrap())
}

fn read_map(root: &std::path::Path) -> String {
    fs::read_to_string(root.join("architecture/index.html")).unwrap()
}

#[test]
fn architecture_html_embeds_current_crate_and_module_svgs_without_external_files() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture(
        "mod leaf { /*! Role: 参照先を提供する。 */ pub fn run() {} }\npub fn run() { leaf::run(); }",
        "",
    );
    add_provider(&directory);
    let output = check_cli(&directory, &node);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = read_map(directory.path());
    let (_, data) = payload(&html);
    assert_eq!(data["packages"], serde_json::json!(["fixture", "provider"]));
    let crates = data["crates"].as_str().unwrap();
    assert_eq!(
        crates,
        fs::read_to_string(directory.path().join("architecture/crates.svg")).unwrap()
    );
    assert!(crates.contains("data-from=\"fixture\" data-to=\"provider\""));
    let modules = data["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 4);
    for package in ["fixture", "provider"] {
        for test in [false, true] {
            let entry = modules
                .iter()
                .find(|entry| entry["package"] == package && entry["test"] == test)
                .unwrap();
            assert_eq!(entry["target"], package);
            assert_eq!(entry["kind"], "lib");
            let mode = if test { "test" } else { "normal" };
            let svg = fs::read_to_string(directory.path().join(format!(
                "architecture/modules/{package}/{package}-lib-{mode}.svg"
            )))
            .unwrap();
            assert_eq!(entry["svg"], svg);
        }
    }
    assert!(data["notices"].as_array().unwrap().is_empty());
    assert!(!html.contains("<!--XTASK_"));
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link rel=\"stylesheet\""));
    assert!(String::from_utf8_lossy(&output.stdout).contains("index.html"));
}

#[test]
fn architecture_html_applies_target_selection_and_is_a_report_artifact() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture("pub fn run() {}", "[diagrams]\ntargets = 'lib'\n");
    fs::write(
        directory.path().join("src/main.rs"),
        "//! Role: 実行ファイルを提供する。\nfn main() {}\n",
    )
    .unwrap();
    let output = check_cli(&directory, &node);
    assert!(output.status.success());
    let (_, data) = payload(&read_map(directory.path()));
    let modules = data["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 2);
    assert!(modules.iter().all(|entry| entry["kind"] == "lib"));
    let report = run(&Config::new(directory.path())).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(
        report
            .artifacts
            .contains(&directory.path().join("architecture/index.html"))
    );
}

#[test]
fn architecture_html_data_cannot_terminate_the_json_script_element() {
    let Some(node) = available_node() else {
        return;
    };
    let directory = fixture("", "");
    let role = "</script><script>alert(1)</script> &";
    fs::write(
        directory.path().join("src/lib.rs"),
        format!("//! Role: {role}\npub fn run() {{}}\n"),
    )
    .unwrap();
    let output = check_cli(&directory, &node);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let html = read_map(directory.path());
    let (json, data) = payload(&html);
    for character in ['<', '>', '&', '\u{2028}', '\u{2029}'] {
        assert!(!json.contains(character), "unescaped {character:?}");
    }
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(
        data["crates"]
            .as_str()
            .unwrap()
            .contains("&lt;/script&gt;&lt;script&gt;alert(1)&lt;/script&gt; &amp;")
    );
}

#[test]
fn architecture_html_omits_retained_module_diagrams_after_analysis_failure() {
    let Some(node) = available_node() else {
        return;
    };
    for policy in ["", "[analysis]\nfeatures = ['fixture/missing']\n"] {
        let directory = fixture("pub fn broken(", policy);
        let modules = directory.path().join("architecture/modules/fixture");
        fs::create_dir_all(&modules).unwrap();
        for mode in ["normal", "test"] {
            fs::write(
                modules.join(format!("fixture-lib-{mode}.svg")),
                "retained stale module sentinel",
            )
            .unwrap();
        }
        let output = check_cli(&directory, &node);
        assert_eq!(output.status.code(), Some(1));
        let html = read_map(directory.path());
        let (_, data) = payload(&html);
        assert!(data["crates"].is_string());
        assert!(data["modules"].as_array().unwrap().is_empty());
        assert!(!data["notices"].as_array().unwrap().is_empty());
        assert!(!html.contains("retained stale module sentinel"));
        for mode in ["normal", "test"] {
            assert_eq!(
                module_svg(directory.path(), mode),
                "retained stale module sentinel"
            );
        }
    }
}

#[test]
fn architecture_html_never_embeds_old_svgs_when_the_renderer_fails() {
    let directory = fixture("pub fn run() {}", "");
    let modules = directory.path().join("architecture/modules/fixture");
    fs::create_dir_all(&modules).unwrap();
    fs::write(
        directory.path().join("architecture/crates.svg"),
        "retained stale crate sentinel",
    )
    .unwrap();
    for mode in ["normal", "test"] {
        fs::write(
            modules.join(format!("fixture-lib-{mode}.svg")),
            "retained stale module sentinel",
        )
        .unwrap();
    }
    let output = check_cli(
        &directory,
        directory.path().join("missing-node-executable").as_os_str(),
    );
    assert_eq!(output.status.code(), Some(1));
    let html = read_map(directory.path());
    let (_, data) = payload(&html);
    assert!(data["crates"].is_null());
    assert_eq!(data["packages"], serde_json::json!(["fixture"]));
    assert!(data["modules"].as_array().unwrap().is_empty());
    assert!(!data["notices"].as_array().unwrap().is_empty());
    assert!(!html.contains("retained stale"));
}

#[test]
fn no_write_preserves_the_current_html_and_does_not_create_a_new_one() {
    let directory = fixture("pub fn run() {}", "");
    let mut config = Config::new(directory.path());
    config.no_write = true;
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(report.artifacts.is_empty());
    assert!(!directory.path().join("architecture").exists());
    fs::create_dir_all(directory.path().join("architecture")).unwrap();
    fs::write(
        directory.path().join("architecture/index.html"),
        "existing architecture HTML sentinel",
    )
    .unwrap();
    let before = immutable_snapshot(directory.path(), &["architecture/index.html"]);
    let report = run(&config).unwrap();
    assert!(report.is_success(), "{:?}", report.errors);
    assert!(report.artifacts.is_empty());
    assert_eq!(
        before,
        immutable_snapshot(directory.path(), &["architecture/index.html"])
    );
}

//! Role: policyの読込み・検証と、解析設定・制限値・出力先・可視性の読出しを検証する。
use std::collections::BTreeSet;
use std::path::Path;

use crate::{AnalysisSettings, DiagramTargets, Features, Policy, StyleRule};

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn old_policies_remain_disabled_and_opt_in_selects_thirteen_rules() {
    assert!(Policy::from_toml("").unwrap().style().is_none());
    let policy = Policy::from_toml("[style]\nenabled=true\n").unwrap();
    let style = policy.style().unwrap();
    assert_eq!(style.rules().len(), 13);
    assert!(style.rules().contains(&StyleRule::Unwrap));
    assert!(!style.rules().contains(&StyleRule::GlobImports));
    for rule in StyleRule::ALL {
        assert_eq!(rule.as_str().parse::<StyleRule>().unwrap(), rule);
    }
}

#[test]
fn all_twenty_rules_can_be_selected_with_explicit_api_targets() {
    let policy = Policy::from_toml(concat!(
        "[style]\nenabled=true\n",
        "extra_rules=['glob-imports','non-leaf-implementation','rustfmt-skip',",
        "'non-exhaustive','catch-unwind','poison-access','options-default']\n",
        "api_packages=['my-api']\n",
        "options_structs=['my-api::Options']\ntoolchain='1.85.0'\n"
    ))
    .unwrap();
    assert_eq!(policy.style().unwrap().rules().len(), 20);
    let policy = Policy::from_toml("[style]\nenabled=true\ndisabled_rules=['format']\n").unwrap();
    assert_eq!(policy.style().unwrap().rules().len(), 12);
}

#[test]
fn invalid_policy_fields_rules_targets_and_exceptions_are_rejected() {
    for setting in [
        "unknown=true",
        "extra_rules=['typo']",
        "extra_rules=['glob-imports','glob-imports']",
        "extra_rules=['glob-imports']\ndisabled_rules=['glob-imports']",
        "extra_rules=['non-exhaustive']",
        "extra_rules=['extension-trait-names']",
        "extension_traits=['my-api::ReadExt']",
        "options_structs=['Options']",
        "options_structs=['api::bad::']",
        "toolchain='--help'",
        "toolchain='../toolchain'",
        "toolchain=''",
        "api_packages=['*']",
    ] {
        let text = format!("[style]\nenabled=true\n{setting}\n");
        assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
    }
    assert!(Policy::from_toml("style=[]\n").is_err());
    for path in [
        "",
        "/src/lib.rs",
        "../src/lib.rs",
        "src/../lib.rs",
        "C:/src/lib.rs",
        "src/*.rs",
        "src/**",
    ] {
        let text = format!(
            "[style]\n[[style.exceptions]]\nrule='unwrap'\npath='{path}'\nreason='reviewed'\n"
        );
        assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
    }
    assert!(
        Policy::from_toml("[[style.exceptions]]\nrule='unwrap'\npath='src/lib.rs'\nreason='  '\n")
            .is_err()
    );
}

#[test]
fn bare_wildcard_exception_rejected() {
    let text = |path: &str| {
        format!("[style]\n[[style.exceptions]]\nrule='unwrap'\npath='{path}'\nreason='reviewed'\n")
    };
    assert!(Policy::from_toml(&text("*")).is_err());
    assert!(Policy::from_toml(&text("src/*")).is_ok());
}

#[test]
fn style_checks_ignore_enabled() {
    let text = "[style]\nenabled=false\nextra_rules=['non-exhaustive']\n";
    let error = Policy::from_toml(text).unwrap_err();
    assert!(error.contains("api_packages"), "{error}");
}

#[test]
fn options_default_can_be_enabled_before_options_types_exist() {
    let policy = Policy::from_toml(
        "[style]\nenabled=true\nextra_rules=['options-default']\noptions_structs=[]\n",
    )
    .unwrap();
    let style = policy.style().unwrap();
    assert!(style.rules().contains(&StyleRule::OptionsDefault));
    assert!(style.options_structs().is_empty());
}

#[test]
fn exceptions_match_exact_rules_paths_and_items_without_broadening() {
    let policy = Policy::from_toml(concat!(
        "[style]\nenabled=true\n",
        "[[style.exceptions]]\nrule='glob-imports'\npath='crates/api/src/*'\n",
        "item='io::tests'\nreason='test fixtures intentionally import their parent'\n"
    ))
    .unwrap();
    let exception = &policy.style().unwrap().exceptions()[0];
    assert!(exception.matches(
        StyleRule::GlobImports,
        "crates\\api\\src\\io.rs",
        Some("io::tests")
    ));
    assert!(!exception.matches(
        StyleRule::GlobImports,
        "crates/api2/src/io.rs",
        Some("io::tests")
    ));
    assert!(!exception.matches(StyleRule::GlobImports, "crates/api/src/io.rs", Some("io")));
    assert!(!exception.matches(StyleRule::Unwrap, "crates/api/src/io.rs", Some("io::tests")));
    assert!(!exception.matches(StyleRule::GlobImports, "crates/api/src/io.rs", None));
}

#[test]
fn policy_defaults_and_patterns_and_unknown_fields_are_strict() {
    let policy = Policy::from_toml("[visible_to]\ncore=['app*']\nprivate=[]\n").unwrap();
    assert_eq!(policy.limits().max_code_lines(), 1000);
    assert_eq!(policy.limits().max_role_chars(), 80);
    assert!(policy.visible("open", "anything"));
    assert!(policy.visible("core", "app"));
    assert!(policy.visible("core", "app-shell"));
    assert!(!policy.visible("core", "other"));
    assert!(!policy.visible("private", "app"));
    assert!(policy.visible("private", "private"));
    for invalid in [
        "[limits]\nmax_code_lines=1001\n",
        "[limits]\nmax_prelude_names=1\n",
        "[dependency_allowlist]\ncore=[]\n",
        "[visible_to]\ncore=['app**']\n",
        "[visible_to]\ncore=['a*p']\n",
        "[visible_to]\ncore=['']\n",
    ] {
        assert!(Policy::from_toml(invalid).is_err(), "accepted {invalid}");
    }
    assert!(Policy::load(Path::new("missing-policy.toml")).is_err());
}

#[test]
fn duplicate_visible_to_rejected() {
    let error = Policy::from_toml("[visible_to]\ncore=['app','app']\n").unwrap_err();
    assert!(error.contains("duplicate"), "{error}");
}

#[test]
fn limits_must_be_a_table() {
    let error = Policy::from_toml("limits = [500, 40]\n").unwrap_err();
    assert!(error.contains("[limits] must be a table"), "{error}");
}

#[test]
fn zero_code_lines_rejected() {
    let error = Policy::from_toml("[limits]\nmax_code_lines=0\n").unwrap_err();
    assert!(error.contains("max_code_lines"), "{error}");
}

#[test]
fn empty_feature_rejected() {
    assert!(Policy::from_toml("[analysis]\nfeatures=['']\n").is_err());
    assert!(Policy::from_toml("[analysis]\nfeatures=[' app ']\n").is_err());
}

#[test]
fn output_moved() {
    let error = Policy::from_toml("output='architecture'\n").unwrap_err();
    assert!(error.contains("[diagrams] output"), "{error}");
    let policy = Policy::from_toml("[diagrams]\noutput='architecture'\n").unwrap();
    assert_eq!(policy.diagrams().output(), Path::new("architecture"));
}

#[test]
fn diagram_targets_default_to_all_and_parse_by_name() {
    use DiagramTargets::{All, Lib, LibBin};
    assert_eq!(Policy::default().diagrams().targets(), All);
    for text in ["", "[diagrams]\n", "[diagrams]\ntargets='all'\n"] {
        assert_eq!(Policy::from_toml(text).unwrap().diagrams().targets(), All);
    }
    let parsed = |value: &str| {
        Policy::from_toml(&format!("[diagrams]\ntargets='{value}'\n"))
            .unwrap()
            .diagrams()
            .targets()
    };
    assert_eq!(parsed("lib"), Lib);
    assert_eq!(parsed("lib-bin"), LibBin);
    for invalid in [
        "[diagrams]\ntargets='tests'\n",
        "[diagrams]\ntargets=['lib']\n",
        "[diagrams]\nkinds='lib'\n",
    ] {
        assert!(Policy::from_toml(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn role_character_limit_defaults_and_accepts_any_positive_setting() {
    assert_eq!(crate::Limits::default().max_role_chars(), 80);
    assert_eq!(Policy::default().limits().max_role_chars(), 80);
    for text in ["", "[limits]\n", "[limits]\nmax_code_lines=25\n"] {
        assert_eq!(
            Policy::from_toml(text).unwrap().limits().max_role_chars(),
            80
        );
    }
    for limit in [1, 8, 80, 160, 1000] {
        let policy = Policy::from_toml(&format!("[limits]\nmax_role_chars={limit}\n")).unwrap();
        assert_eq!(policy.limits().max_role_chars(), limit);
        assert_eq!(policy.limits().max_code_lines(), 1000);
    }
    let policy = Policy::from_toml(
        "[limits]\nmax_code_lines=20\nmax_role_chars=160\n[visible_to]\ncore=['app*']\n",
    )
    .unwrap();
    assert_eq!(policy.limits().max_code_lines(), 20);
    assert_eq!(policy.limits().max_role_chars(), 160);
    assert!(policy.visible("core", "app-shell"));
    assert!(!policy.visible("core", "other"));
    assert!(Policy::from_toml("[limits]\nmax_code_lines=1001\nmax_role_chars=160\n").is_err());
}

#[test]
fn role_character_limit_rejects_zero_negative_and_non_integer_values() {
    for value in ["0", "-1", "1.5", "'80'", "true", "[]"] {
        let text = format!("[limits]\nmax_role_chars={value}\n");
        assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
    }
    let error = Policy::from_toml("[limits]\nmax_role_chars=0\n").unwrap_err();
    assert!(error.contains("max_role_chars"), "{error}");
}

#[test]
fn analysis_settings_default_to_host_with_cargo_default_features() {
    let expected = AnalysisSettings::new(
        None,
        Features::Selected {
            features: BTreeSet::new(),
            default_features: true,
        },
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    assert_eq!(AnalysisSettings::default(), expected);
    assert_eq!(Policy::default().analysis(), &expected);
    for text in ["", "[analysis]\n", "[limits]\nmax_code_lines=20\n"] {
        assert_eq!(Policy::from_toml(text).unwrap().analysis(), &expected);
    }
    let policy = Policy::from_toml("[analysis]\nfeatures=['app/feature']\n").unwrap();
    assert_eq!(
        policy.analysis().features(),
        &Features::Selected {
            features: names(&["app/feature"]),
            default_features: true,
        }
    );
    assert!(policy.analysis().target().is_none());
}

#[test]
fn analysis_settings_parse_cargo_cfg_and_trusted_expansions_with_other_rules() {
    // STOP: this input has `all_features` with `features`, and repeated trusted names. The
    // contract (M1 step 4) rejects both, so this test's expectation cannot hold.
    let policy = Policy::from_toml(concat!(
        "[analysis]\ntarget='x86_64-pc-windows-msvc'\n",
        "features=['app/left','right']\nall_features=true\ndefault_features=false\n",
        "cfg=['custom','platform=\"demo\"']\n",
        "trusted_attributes=['macro_lib::second','macro_lib::first','macro_lib::second']\n",
        "trusted_derives=['derive_lib::Marker']\n",
        "trusted_macros=['proptest','prop_assert','proptest']\n",
        "trusted_globs=['framework::prelude']\n",
        "[limits]\nmax_code_lines=25\nmax_role_chars=160\n",
        "[visible_to]\ncore=['app*']\n"
    ))
    .unwrap();
    let analysis = policy.analysis();
    assert_eq!(analysis.target(), Some("x86_64-pc-windows-msvc"));
    assert_eq!(analysis.cfg(), &names(&["custom", "platform=\"demo\""]));
    assert_eq!(
        analysis.trusted_attributes(),
        &names(&["macro_lib::first", "macro_lib::second"])
    );
    assert_eq!(analysis.trusted_derives(), &names(&["derive_lib::Marker"]));
    assert_eq!(
        analysis.trusted_macros(),
        &names(&["prop_assert", "proptest"])
    );
    assert_eq!(analysis.trusted_globs(), &names(&["framework::prelude"]));
    assert_eq!(policy.limits().max_code_lines(), 25);
    assert_eq!(policy.limits().max_role_chars(), 160);
    assert!(policy.visible("core", "app-shell"));
}

#[test]
fn analysis_settings_reject_unknown_fields_and_incorrect_types() {
    for setting in [
        "tests=false",
        "unknown=[]",
        "target=42",
        "target=['host']",
        "features='left'",
        "features=[1]",
        "all_features=1",
        "default_features='false'",
        "cfg='custom'",
        "cfg=[true]",
        "trusted_attributes='macro_lib::attr'",
        "trusted_attributes=[1]",
        "trusted_derives=true",
        "trusted_derives=[false]",
        "trusted_macros='proptest'",
        "trusted_macros=[1]",
    ] {
        let text = format!("[analysis]\n{setting}\n");
        assert!(Policy::from_toml(&text).is_err(), "accepted {text}");
    }
    assert!(Policy::from_toml("analysis=[]\n").is_err());
}

#[test]
fn all_features_cannot_combine_with_explicit_features() {
    let error = Policy::from_toml("[analysis]\nall_features=true\nfeatures=['app']\n").unwrap_err();
    assert!(error.contains("all_features cannot be combined"), "{error}");
    let error =
        Policy::from_toml("[analysis]\nall_features=true\ndefault_features=true\n").unwrap_err();
    assert!(error.contains("all_features cannot be combined"), "{error}");
    let policy = Policy::from_toml("[analysis]\nall_features=true\n").unwrap();
    assert_eq!(policy.analysis().features(), &Features::All);
}

#[test]
fn diagram_output_defaults_with_and_without_other_policy_tables() {
    let expected = Path::new("target/xtask/architecture.xtaskmap");
    assert_eq!(Policy::default().diagrams().output(), expected);
    for text in [
        "",
        "[limits]\nmax_role_chars=160\n",
        "[analysis]\nfeatures=['left']\n",
        "[visible_to]\ncore=['app*']\n",
        "[limits]\nmax_code_lines=25\n[analysis]\ndefault_features=false\n[visible_to]\ncore=[]\n",
    ] {
        assert_eq!(
            Policy::from_toml(text).unwrap().diagrams().output(),
            expected
        );
    }
}

#[test]
fn diagram_output_accepts_relative_absolute_and_space_containing_paths() {
    for path in [
        "docs/architecture",
        "docs/SVG output",
        " docs/architecture ",
    ] {
        let policy = Policy::from_toml(&format!("[diagrams]\noutput='{path}'\n")).unwrap();
        assert_eq!(policy.diagrams().output(), Path::new(path));
    }
    let absolute = std::env::temp_dir().join("SVG output");
    assert!(absolute.is_absolute());
    let policy = Policy::from_toml(&format!(
        "[diagrams]\noutput='{}'\n[limits]\nmax_role_chars=160\n[analysis]\nall_features=true\n[visible_to]\ncore=['app*']\n",
        absolute.display()
    ))
    .unwrap();
    assert_eq!(policy.diagrams().output(), absolute);
    assert_eq!(policy.limits().max_role_chars(), 160);
    assert_eq!(policy.analysis().features(), &Features::All);
    assert!(policy.visible("core", "app-shell"));
}

#[test]
fn diagram_output_rejects_blank_paths_wrong_types_and_unknown_fields() {
    for text in [
        "[diagrams]\noutput=''\n",
        "[diagrams]\noutput='  '\n",
        "[diagrams]\noutput=\"\\t\\n \"\n",
        "[diagrams]\noutput=12\n",
        "[diagrams]\noutput=true\n",
        "[diagrams]\noutput=[]\n",
        "[diagrams]\noutput=['architecture']\n",
        "[output]\npath='architecture'\n",
        "[diagrams]\noutput='architecture'\nunknown=true\n",
    ] {
        assert!(Policy::from_toml(text).is_err(), "accepted {text}");
    }
}

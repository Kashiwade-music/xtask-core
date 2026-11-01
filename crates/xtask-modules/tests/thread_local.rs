//! Role: Verify standard thread-local static names without expanding other macros.
use std::fs;
use xtask_modules::{AnalysisOptions, ModuleGraph, analyze};

fn graph(source: &str) -> ModuleGraph {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path().join("lib.rs");
    fs::write(&root, source).unwrap();
    let mut options = AnalysisOptions::default();
    for name in [
        "thread_local",
        "std::thread_local",
        "::std::thread_local",
        "tls",
        "other",
    ] {
        options.trusted_macros.insert(name.into());
    }
    analyze(&root, &options).unwrap()
}

#[test]
fn standard_statics_resolve_imports_aliases_reexports_and_edges() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: Thread-local declarations.
    std::thread_local! {
        #[doc = "A constant initializer."]
        pub static FIRST: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
        pub static SECOND: std::cell::Cell<u8> = std::cell::Cell::new(1)
    }
    thread_local! { pub static THIRD: u8 = 3; }
    ::std::thread_local! { pub static FOURTH: u8 = 4; }
}
mod facade {
    //! Role: Static reexports.
    pub use crate::declarations::{FIRST as RENAMED, SECOND, THIRD, FOURTH};
}
mod consumer {
    //! Role: Static consumers.
    use crate::facade::{RENAMED, SECOND, THIRD, FOURTH};
    fn read() { RENAMED.with(|_| ()); SECOND.with(|_| ()); THIRD.with(|_| ()); FOURTH.with(|_| ()); }
}
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for from in ["crate::facade", "crate::consumer"] {
        assert!(
            result
                .edges
                .iter()
                .any(|edge| edge.from == from && edge.to == "crate::declarations")
        );
    }
}

#[test]
fn standard_macro_alias_resolves_after_reexport() {
    let result = graph(
        r#"
//! Role: Test root.
mod macros {
    //! Role: Standard macro reexport.
    pub use std::thread_local as tls;
}
mod declarations {
    //! Role: Aliased standard invocation.
    use crate::macros::tls;
    tls! { pub static VALUE: u8 = 1; }
}
mod consumer {
    //! Role: Generated static import.
    use crate::declarations::VALUE;
}
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result
            .edges
            .iter()
            .any(|edge| edge.from == "crate::consumer" && edge.to == "crate::declarations")
    );
}

#[test]
fn missing_and_inactive_statics_still_report_unresolved_imports() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: Conditionally absent declarations.
    thread_local! {
        #[cfg(any())] pub static DISABLED: u8 = 0;
        #[cfg_attr(all(), cfg(any()))] pub static ALSO_DISABLED: u8 = 0;
        pub static ACTIVE: u8 = 1;
    }
    #[cfg(any())] thread_local! { pub static WHOLE_DISABLED: u8 = 0; }
}
mod consumer {
    //! Role: Invalid static imports.
    use crate::declarations::{ACTIVE, MISSING, DISABLED, ALSO_DISABLED, WHOLE_DISABLED};
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 4, "{:?}", result.diagnostics);
    for name in ["MISSING", "DISABLED", "ALSO_DISABLED", "WHOLE_DISABLED"] {
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.ends_with(name))
        );
    }
}

#[test]
fn other_macro_providers_do_not_gain_static_declarations() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: Nonstandard macro providers.
    use external::thread_local;
    thread_local! { pub static FAKE: u8 = 1; }
    other! { pub static ALSO_FAKE: u8 = 2; }
}

mod consumer {
    //! Role: Invalid generated name imports.
    use crate::declarations::{FAKE, ALSO_FAKE};
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.ends_with("::FAKE"))
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.ends_with("::ALSO_FAKE"))
    );
}

#[test]
fn local_std_module_is_not_the_standard_macro_provider() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: A shadowed standard-library name.
    mod std {
        //! Role: A local standard-library namesake.
        pub fn thread_local() {}
    }
    std::thread_local! { pub static FAKE: u8 = 1; }
}
mod consumer {
    //! Role: Invalid generated static import.
    use crate::declarations::FAKE;
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].ends_with("::FAKE"));
}

#[test]
fn delayed_external_glob_provider_does_not_leave_a_generated_static() {
    let result = graph(
        r#"
//! Role: Test root.
mod provider {
    //! Role: External macro reexport.
    pub use external::thread_local;
}
mod declarations {
    //! Role: Delayed external glob import.
    use crate::provider::*;
    thread_local! { pub static FAKE: u8 = 1; }
}
mod consumer {
    //! Role: Invalid generated static import.
    use crate::declarations::FAKE;
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].ends_with("::FAKE"));
}

#[test]
fn value_names_do_not_shadow_standard_macro_or_module_namespaces() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: Values coexist with standard macro paths.
    fn thread_local() {}
    const std: u8 = 1;
    thread_local! { pub static FIRST: u8 = 1; }
    std::thread_local! { pub static SECOND: u8 = 2; }
}
mod consumer {
    //! Role: Valid generated static imports.
    use crate::declarations::{FIRST, SECOND};
}
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result
            .edges
            .iter()
            .any(|edge| edge.from == "crate::consumer" && edge.to == "crate::declarations")
    );
}

#[test]
fn unresolved_explicit_providers_and_globs_do_not_create_statics() {
    for import in [
        "use crate::missing::thread_local;",
        "use crate::missing::provider as thread_local;",
        "use crate::missing::provider as std;",
        "use crate::missing::*;",
    ] {
        let source = format!(
            r#"
//! Role: Test root.
mod declarations {{
    //! Role: Unresolved macro provider.
    {import}
    thread_local! {{ pub static FIRST: u8 = 1; }}
    std::thread_local! {{ pub static SECOND: u8 = 2; }}
}}
mod consumer {{
    //! Role: Invalid generated static imports.
    use crate::declarations::{{FIRST, SECOND}};
}}
"#
        );
        let result = graph(&source);
        let unresolved = if import.contains(" as std") {
            "::SECOND"
        } else {
            "::FIRST"
        };
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.ends_with(unresolved)),
            "{import}: {:?}",
            result.diagnostics
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("crate::missing")),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn resolved_value_imports_and_inactive_imports_preserve_standard_providers() {
    let result = graph(
        r#"
//! Role: Test root.
mod values {
    //! Role: Values sharing standard provider names.
    pub fn thread_local() {}
    pub const std: u8 = 1;
}
mod declarations {
    //! Role: Value imports do not occupy provider namespaces.
    use crate::values::{thread_local, std};
    #[cfg(any())] use crate::missing::thread_local;
    #[cfg(any())] use crate::missing::provider as std;
    #[cfg(any())] use crate::missing::*;
    thread_local! { pub static FIRST: u8 = 1; }
    std::thread_local! { pub static SECOND: u8 = 2; }
}
mod consumer {
    //! Role: Valid generated static imports.
    use crate::declarations::{FIRST, SECOND};
}
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn absolute_standard_provider_ignores_unresolved_local_std_alias() {
    let result = graph(
        r#"
//! Role: Test root.
mod declarations {
    //! Role: Absolute provider with unresolved local alias.
    use crate::missing::provider as std;
    ::std::thread_local! { pub static VALUE: u8 = 1; }
}
mod consumer {
    //! Role: A valid absolute-provider static import.
    use crate::declarations::VALUE;
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].contains("crate::missing"));
}

#[test]
fn unresolved_imports_behind_local_globs_remain_uncertain() {
    let result = graph(
        r#"
//! Role: Test root.
mod provider {
    //! Role: Unresolved macro reexport.
    pub use crate::missing::thread_local;
}
mod declarations {
    //! Role: Local glob with unresolved provider names.
    use crate::provider::*;
    thread_local! { pub static FAKE: u8 = 1; }
}
mod consumer {
    //! Role: Invalid generated static import.
    use crate::declarations::FAKE;
}
"#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.ends_with("::FAKE"))
    );
}

#[test]
fn unknown_external_macro_use_does_not_infer_bare_standard_provider() {
    let result = graph(
        r#"
//! Role: Test root.
#[macro_use] extern crate external;
mod declarations {
    //! Role: Unknown macro-use provider.
    thread_local! { pub static FAKE: u8 = 1; }
    ::std::thread_local! { pub static ACTUAL: u8 = 2; }
}
mod consumer {
    //! Role: Macro-use uncertainty and absolute standard path.
    use crate::declarations::{FAKE, ACTUAL};
}
"#,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].ends_with("::FAKE"));
}

#[test]
fn effective_macro_use_attributes_only_block_selected_macro_names() {
    for (attribute, blocks) in [
        ("#[cfg_attr(all(), macro_use)]", true),
        ("#[cfg_attr(all(), macro_use(thread_local,))]", true),
        ("#[macro_use(other, thread_local)]", true),
        ("#[macro_use(r#thread_local,)]", true),
        ("#[macro_use(other::unknown)]", true),
        ("#[macro_use(other)]", false),
        ("#[macro_use()]", false),
        ("#[cfg(any())] #[macro_use]", false),
        ("#[cfg_attr(any(), macro_use)]", false),
        ("#[cfg_attr(any(), macro_use(thread_local))]", false),
    ] {
        let source = format!(
            r#"
//! Role: Test root.
{attribute} extern crate external;
mod declarations {{
    //! Role: Effective macro-use provider boundaries.
    thread_local! {{ pub static BARE: u8 = 1; }}
    std::thread_local! {{ pub static QUALIFIED: u8 = 2; }}
    ::std::thread_local! {{ pub static ABSOLUTE: u8 = 3; }}
}}
mod consumer {{
    //! Role: Imported generated static names.
    use crate::declarations::{{BARE, QUALIFIED, ABSOLUTE}};
}}
"#
        );
        let result = graph(&source);
        assert_eq!(
            result.diagnostics.len(),
            usize::from(blocks),
            "{attribute}: {:?}",
            result.diagnostics
        );
        if blocks {
            assert!(
                result.diagnostics[0].ends_with("::BARE"),
                "{:?}",
                result.diagnostics
            );
        }
    }
}

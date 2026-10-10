//! Role: コード行数の計測、ソース探索、読み取り失敗の扱いを検証する。
use std::fs;
use xtask_source::{check_files, code_line_count, rust_files};

#[test]
fn physical_lines_exclude_nested_comments_and_string_contents() {
    let source = concat!(
        "//! Module documentation.\n",
        "/* outer\n /* inner */\n end */\n",
        "fn main() {\n",
        "    let text = \"\n",
        "// This is literal content.\n",
        "/* This is also literal content. */\n",
        "    \";\n",
        "}\n",
    );
    assert_eq!(code_line_count(source), 4);
}

#[test]
fn escaped_newline_keeps_the_closing_delimiter_on_a_separate_line() {
    let source =
        "fn main() {\nlet s = \"open\\\n\";\nlet t = \"open\\\r\n    interior\\\n    \";\n}\n";
    // fn, first opener, first closer, second opener, second closer, closing brace.
    assert_eq!(code_line_count(source), 6);
}

#[test]
fn raw_literals_with_embedded_delimiters_and_prefixes() {
    let source = concat!(
        "fn main() {\n",
        "let text = r###\"\n",
        "a \"## almost-close // comment\n",
        "\"###;\n",
        "let bytes = br##\"\n",
        "b \"# almost-close /* comment */\n",
        "\"##;\n",
        "let c = cr#\"\n",
        "c string // interior\n",
        "\"#;\n",
        "let c = c\"\n",
        "ordinary C string /* interior */\n",
        "\";\n",
        "}\n",
    );
    assert_eq!(code_line_count(source), 10);
}

#[test]
fn lifetimes_labels_unicode_and_escaped_characters_do_not_hide_following_code() {
    let source = concat!(
        "fn text<'寿>(s: &'寿 str) -> &'寿 str {\n",
        "    'outer: loop {\n",
        "        let chars = ['界', '\\u{1f980}', '\\'', '\\\\', '\"'];\n",
        "        let bytes = [b'\\n', b'\\x2f', b'\\''];\n",
        "        break 'outer; // tail\n",
        "    }\n",
        "    s\n",
        "}\n",
    );
    assert_eq!(code_line_count(source), 8);
}

#[test]
fn doc_comments_are_excluded_but_explicit_doc_attributes_count() {
    let source = concat!(
        "//! inner doc\n",
        "/*! block inner\n",
        "   doc */\n",
        "/// outer doc\n",
        "/** block outer\n",
        "    doc */\n",
        "#[doc = \"\n",
        "explicit literal content\n",
        "\"]\n",
        "fn main() {}\n",
    );
    assert_eq!(code_line_count(source), 3);
}

#[test]
fn group_spans_do_not_mark_comment_only_interior_lines() {
    let source = "fn main(\n/* arguments */\n) {\n// body\n{\n/* nested */\n}\n}\n";
    assert_eq!(code_line_count(source), 5);
}

#[test]
fn unicode_doc_span_and_file_preamble_keep_physical_lines() {
    assert_eq!(
        code_line_count(
            "\u{feff}#!/usr/bin/env rust-script\n//! 寿\nfn main() {\nlet 界 = \"\ninside\n\";\n}\n"
        ),
        4
    );
    assert_eq!(code_line_count("#![allow(dead_code)]\nfn main() {}"), 2);
    assert_eq!(
        code_line_count("#! /* outer /* inner */ end */ [allow(dead_code)]\nfn main() {}"),
        2
    );
    assert_eq!(
        code_line_count("#! // ordinary comment\n[allow(dead_code)]\nfn main() {}"),
        3
    );
}

#[test]
fn empty_or_comment_only_files_have_zero_code_lines() {
    for source in [
        "",
        " \n\t\r\n",
        "// comment",
        "/* nested /* yes */ */",
        "/// doc",
    ] {
        assert_eq!(code_line_count(source), 0, "{source:?}");
    }
}

#[test]
fn discovery_includes_all_conventional_roots_and_build_script_in_order() {
    let temp = tempfile::tempdir().unwrap();
    for root in ["src/nested", "tests", "benches", "examples"] {
        fs::create_dir_all(temp.path().join(root)).unwrap();
        fs::write(temp.path().join(root).join("test.rs"), "fn main() {}").unwrap();
        fs::write(temp.path().join(root).join("ignore.txt"), "ignored").unwrap();
    }
    fs::write(temp.path().join("build.rs"), "fn main() {}").unwrap();
    fs::write(temp.path().join("unconfigured.rs"), "fn main() {}").unwrap();
    let files = rust_files(temp.path()).unwrap();
    assert_eq!(files.len(), 5);
    assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(files.contains(&temp.path().join("build.rs")));
    assert!(!files.contains(&temp.path().join("unconfigured.rs")));
}

#[test]
fn limit_is_inclusive_and_files_are_deduplicated() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("test.rs");
    fs::write(&file, "fn main() {\nlet x = 1;\n}\n").unwrap();
    assert!(
        check_files(std::slice::from_ref(&file), 3)
            .unwrap()
            .is_empty()
    );
    let violations = check_files(&[file.clone(), file.clone()], 2).unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].path, file);
    assert_eq!(violations[0].count, 3);
    assert_eq!(violations[0].limit, 2);
    let alternate = temp.path().join("./test.rs");
    let canonical = fs::canonicalize(&file).unwrap();
    let violations = check_files(&[file, alternate, canonical], 2).unwrap();
    assert_eq!(
        violations.len(),
        1,
        "physical file identity must be deduplicated"
    );
}

#[test]
fn malformed_or_unreadable_files_are_errors() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("test.rs");
    fs::write(&file, "fn main() {\nlet s = \"unterminated\n").unwrap();
    assert!(
        check_files(std::slice::from_ref(&file), 1000)
            .unwrap_err()
            .contains("cannot tokenize")
    );
    assert_eq!(code_line_count("fn main() {\nlet s = \"unterminated\n"), 2);
    fs::write(&file, [0xff]).unwrap();
    assert!(check_files(std::slice::from_ref(&file), 1000).is_err());
    fs::remove_file(&file).unwrap();
    assert!(check_files(&[file], 1000).is_err());
    assert!(rust_files(&temp.path().join("missing")).is_err());
}

#[cfg(unix)]
#[test]
fn directory_symlink_cycles_terminate_without_omitting_original_files() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("lib.rs"), "fn main() {}").unwrap();
    std::os::unix::fs::symlink(&src, src.join("loop")).unwrap();
    assert_eq!(rust_files(temp.path()).unwrap(), vec![src.join("lib.rs")]);
}

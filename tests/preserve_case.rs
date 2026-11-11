//! `out.*` paths lowercase tag names and attribute keys by default;
//! `GelDocument::preserve_case` keeps their case.

use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::ast::GelDocument;
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;
use rustine::StreamingRunner;

/// Every path-taking `out.*` function, plus the two that never touch case.
const ALL_FUNCTIONS: &str = r#"
grammar input:
    out.set_root_name('Database')
    match /(\w+) (\w+)\n/:
        out.create('Entry?Lang="fr"', '$1')
        out.add('Group/LX', '$2')
        out.replace('Note', '$2')
        out.add_attribute('Group', 'DatabaseType', '$1')
        out.enqueue_after(/ab/, 'Trigger/Path', 'fired')
        out.open('OpenNode')
        out.enter('EnterNode')
"#;

/// (default output, output with preserve_case) per function.
const EXPECTED: &[(&str, &str)] = &[
    (r#"<entry lang="fr">ab</entry>"#, r#"<Entry Lang="fr">ab</Entry>"#),
    (r#"<group DatabaseType="ab">"#, r#"<Group DatabaseType="ab">"#),
    ("<lx>cd</lx>", "<LX>cd</LX>"),
    ("<note>cd</note>", "<Note>cd</Note>"),
    ("<opennode>", "<OpenNode>"),
    ("<enternode", "<EnterNode"),
    ("<trigger>", "<Trigger>"),
    ("<path>fired</path>", "<Path>fired</Path>"),
];

fn compile(grammar: &str, preserve_case: bool) -> GelDocument {
    let tokens = lex(grammar).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    doc.preserve_case = preserve_case;
    doc
}

fn run_xml(grammar: &str, input: &str, preserve_case: bool) -> String {
    let mut doc = compile(grammar, preserve_case);
    let exec = execute(&mut doc, "input", input).expect("execute");
    serialize_tree(&exec, RuntimeFormat::Xml)
}

#[test]
fn default_lowercases_out_paths() {
    let xml = run_xml(ALL_FUNCTIONS, "ab cd\n", false);
    assert!(xml.contains("<Database>"), "set_root_name keeps case: {xml}");
    for (lower, preserved) in EXPECTED {
        assert!(xml.contains(lower), "missing {lower}: {xml}");
        assert!(!xml.contains(preserved), "unexpected {preserved}: {xml}");
    }
}

#[test]
fn preserve_case_keeps_out_paths() {
    let xml = run_xml(ALL_FUNCTIONS, "ab cd\n", true);
    assert!(xml.contains("<Database>"), "set_root_name keeps case: {xml}");
    for (lower, preserved) in EXPECTED {
        assert!(xml.contains(preserved), "missing {preserved}: {xml}");
        assert!(!xml.contains(lower), "unexpected {lower}: {xml}");
    }
}

#[test]
fn preserve_case_keeps_differently_cased_nodes_apart() {
    let grammar =
        "grammar input:\n    match /(\\w+)\\n/:\n        out.add('Item', '$1')\n        out.add('item', '$1')\n";
    let merged = run_xml(grammar, "x\n", false);
    assert!(merged.contains("<item>xx</item>"), "{merged}");
    let separate = run_xml(grammar, "x\n", true);
    assert!(separate.contains("<Item>x</Item>"), "{separate}");
    assert!(separate.contains("<item>x</item>"), "{separate}");
}

#[test]
fn streaming_runner_honors_preserve_case() {
    let grammar = "grammar input:\n    match /(\\w+)\\n/:\n        out.add('MixedCase', '$1')\n";
    for (preserve_case, expected) in [
        (false, "<mixedcase>value</mixedcase>"),
        (true, "<MixedCase>value</MixedCase>"),
    ] {
        let mut doc = compile(grammar, preserve_case);
        let mut runner = StreamingRunner::new(&mut doc, "input").expect("streaming runner");
        runner.feed("value\n");
        let result = runner.finish().expect("finish");
        let xml = serialize_tree(&result, RuntimeFormat::Xml);
        assert!(xml.contains(expected), "{xml}");
    }
}

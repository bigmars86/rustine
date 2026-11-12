use rustine::parse_and_run;

#[test]
fn test_basic_execution() {
    let gel = "grammar main:\n    match /Hello/:\n        do.say(\"world\")\n        out.create(\"root\")\n";
    let json = parse_and_run(gel, "main", "Hello!").expect("execution failed");
    // Sanity checks: consumed, actions, traces, output all present
    assert!(json.contains("\"consumed\""), "consumed missing: {json}");
    assert!(json.contains("do.say"), "do.say action missing: {json}");
    assert!(json.contains("out.create"), "out.create action missing: {json}");
    assert!(json.contains("\"traces\""), "traces missing: {json}");
    assert!(json.contains("\"root\""), "root node missing in output: {json}");
}

#[test]
fn execute_tree_skips_metadata_only() {
    use rustine::exec::{execute_precompiled, execute_tree, serialize_tree, RuntimeFormat};
    let gel = "grammar sub:\n    match /(\\w+)\\n/:\n        out.add('item', '$1')\n\ngrammar input:\n    match /list\\n/:\n        out.enter('list')\n        sub()\n";
    let input = "list\nalpha\nbeta\n";
    let mut doc = rustine::parser::syntax::parse_gel_document(&rustine::parser::lexer::lex(gel).unwrap()).unwrap();
    doc.compile_regexes();
    let full = execute_precompiled(&doc, "input", input).unwrap();
    let tree = execute_tree(&doc, "input", input).unwrap();
    assert_eq!(
        serialize_tree(&tree, RuntimeFormat::Json).unwrap(),
        serialize_tree(&full, RuntimeFormat::Json).unwrap()
    );
    assert_eq!(tree.consumed, full.consumed);
    assert!(!full.actions.is_empty() && !full.traces.is_empty());
    assert!(tree.actions.is_empty() && tree.traces.is_empty() && tree.capture_history.is_empty());
}

use rustine::parse_and_run;
use serde_json::{json, Value};

fn run(src: &str, input: &str) -> Value {
    let result: Value = serde_json::from_str(&parse_and_run(src, "input", input).expect("run")).expect("json");
    result["output"].clone()
}

/// A path from an empty capture makes `out.create` / `out.add` / `out.replace` do nothing.
#[test]
fn empty_path_does_nothing() {
    let src = "\
define nl /\\n/

grammar input:
    match /y*/ nl:
        out.create('$0', 'c')
        out.add('$0', 'a')
        out.replace('$0', 'r')
        out.add('/', 's')
        out.add('kept', 'k')
";
    assert_eq!(run(src, "\n"), json!({"kept": {"#text": "k"}}));
}

/// With an `on_add` trigger queued, an empty-path `out.add` must not leave the entered node.
#[test]
fn empty_path_keeps_cursor_with_on_add_trigger() {
    let src = "\
define nl /\\n/

grammar input:
    match 'go' /y*/ nl:
        out.enqueue_on_add(/never/, 'fired')
        out.enter('a')
        out.add('$1', 'x')
        out.add('b', 'y')
";
    assert_eq!(run(src, "go\n"), json!({"a": {"b": {"#text": "y"}}}));
}

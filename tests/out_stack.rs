use rustine::parse_and_run;

#[test]
fn test_open_enter_stack_leaves() {
    let gel = "grammar main:\n    match 'abc':\n        out.open('root')\n        out.enter('root/child')\n        out.add('root/child','data')\n";
    let res = parse_and_run(gel, "main", "abc").expect("run");
    let output_idx = res.find("\"output\"").expect("output missing");
    let output_section = &res[output_idx..];
    assert!(output_section.contains("\"root\""), "root missing: {res}");
    assert!(output_section.contains("\"child\""), "child missing: {res}");
    assert!(output_section.contains("data"), "data missing: {res}");
}

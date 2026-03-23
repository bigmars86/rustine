use rustine::parse_and_run;

#[test]
fn test_open_enter_leave_persistence() {
    let gel = "grammar main:\n    match 'xyz':\n        out.open('root')\n        out.enter('root/child')\n        out.add('root/child','data1')\n        out.leave()\n        out.add('root/child','data2')\n";
    let res = parse_and_run(gel, "main", "xyz").expect("run");
    // After enter + add + leave + add, child should exist with text data
    let output_idx = res.find("\"output\"").expect("output missing");
    let output_section = &res[output_idx..];
    assert!(output_section.contains("\"root\""), "root missing: {res}");
    assert!(output_section.contains("\"child\""), "child missing: {res}");
    assert!(output_section.contains("data1"), "data1 missing: {res}");
    assert!(output_section.contains("data2"), "data2 missing: {res}");
}

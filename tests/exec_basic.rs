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

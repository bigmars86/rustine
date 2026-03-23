use rustine::exec::streaming::StreamingEvent;
use rustine::parser::ast::GelDocument;
use rustine::StreamingRunner;

fn parse_doc(src: &str) -> GelDocument {
    let tokens = rustine::parser::lexer::lex(src).unwrap();
    rustine::parser::syntax::parse_gel_document(&tokens).unwrap()
}

#[test]
fn streaming_single_chunk_matches_batch() {
    let src = "\
grammar main:
    match /(\\w+)/:
        out.create(\"word\", $1)
";
    let input = "hello world";
    let mut doc = parse_doc(src);

    // Batch execution
    let batch = rustine::exec::execute(&mut doc.clone(), "main", input).unwrap();

    // Streaming: feed all at once
    let mut sr = StreamingRunner::new(&mut doc, "main").unwrap();
    sr.feed(input);
    sr.set_eof();
    let result = sr.finish().unwrap();

    assert_eq!(result.consumed, batch.consumed, "consumed bytes should match");
}

#[test]
fn streaming_chunked_input() {
    let src = "\
grammar main:
    match /(\\w+)/:
        out.create(\"word\", $1)
    skip /\\s+/
";
    let mut doc = parse_doc(src);
    let mut sr = StreamingRunner::new(&mut doc, "main").unwrap();

    // Feed in two chunks
    sr.feed("hello ");
    let events1 = sr.step().unwrap();
    // Should have matched "hello" and consumed the space
    let has_match = events1
        .iter()
        .any(|e| matches!(e, StreamingEvent::MatchConsumed { .. }));
    assert!(has_match, "first chunk should produce a match event");

    // Feed more data + EOF
    sr.feed("world");
    sr.set_eof();
    let events2 = sr.step().unwrap();
    let has_match2 = events2
        .iter()
        .any(|e| matches!(e, StreamingEvent::MatchConsumed { .. }));
    assert!(has_match2, "second chunk should produce a match event");

    let has_finished = events2.iter().any(|e| matches!(e, StreamingEvent::Finished { .. }));
    assert!(has_finished, "should finish after eof");
}

#[test]
fn streaming_events_include_create() {
    let src = "\
grammar main:
    match /(\\w+)/:
        out.create(\"item\", $1)
    skip /\\s+/
";
    let mut doc = parse_doc(src);
    let mut sr = StreamingRunner::new(&mut doc, "main").unwrap();
    sr.feed("alpha beta");
    sr.set_eof();
    let events = sr.step().unwrap();

    let creates: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, StreamingEvent::Create { .. }))
        .collect();
    assert!(
        creates.len() >= 2,
        "should have at least 2 Create events, got {}",
        creates.len()
    );
}

#[test]
fn streaming_stalled_without_eof() {
    let src = "\
grammar main:
    match /(\\d+)/:
        out.create(\"num\", $1)
";
    let mut doc = parse_doc(src);
    let mut sr = StreamingRunner::new(&mut doc, "main").unwrap();
    sr.feed("abc");
    let events = sr.step().unwrap();
    // Input doesn't match grammar, no EOF → stalled
    let has_stalled = events.iter().any(|e| matches!(e, StreamingEvent::Stalled { .. }));
    assert!(has_stalled, "should stall when no match and no eof");
}

#[test]
fn streaming_finish_produces_full_result() {
    let src = "\
grammar main:
    match /([a-z]+)=([0-9]+)/:
        out.create(\"pair/$1\", $2)
    skip /\\s+/
";
    let mut doc = parse_doc(src);
    let mut sr = StreamingRunner::new(&mut doc, "main").unwrap();
    sr.feed("x=1 y=2 z=3");
    sr.set_eof();
    let result = sr.finish().unwrap();

    assert!(result.consumed > 0, "should consume some input");
    assert!(
        result.capture_history.len() >= 3,
        "should have 3 capture rounds, got {}",
        result.capture_history.len()
    );
}

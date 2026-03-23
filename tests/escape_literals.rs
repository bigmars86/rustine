use rustine::parser::lexer::{lex, TokenKind};
use rustine::stream::{ChunkReader, StreamingLexer};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[test]
fn escaped_quotes_in_string() {
    let input = "define msg \"He said: \\\"Hello\\\"\""; // string with escaped quotes
    let toks = lex(input).unwrap();
    assert!(toks
        .iter()
        .any(|t| t.kind == TokenKind::String && t.slice.contains("\\\"Hello\\\"")));
}

#[test]
fn escaped_slash_in_regex() {
    let input = r"define r /foo\/bar/"; // regex containing escaped slash
    let toks = lex(input).unwrap();
    assert!(toks
        .iter()
        .any(|t| t.kind == TokenKind::Regex && t.slice == "/foo\\/bar/"));
}

fn write_temp(contents: &str, chunk: usize) -> (PathBuf, usize) {
    let path = std::env::temp_dir().join("gel_escape_stream.gel");
    let mut f = File::create(&path).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    (path, chunk)
}

#[test]
fn streaming_split_with_escape() {
    // Split right after an escape backslash to ensure we don't mis-detect termination.
    // We want chunk boundary after the escape backslash but before following quote.
    let source = "define msg \"Hello\\\"World\""; // sequence: Hello \" World
    let (path, chunk) = write_temp(source, 6); // small chunk to split inside literal
    let reader = ChunkReader::open(&path, chunk).unwrap();
    let mut lex = StreamingLexer::new(reader);
    let mut found = false;
    while let Some(batch) = lex.next_batch().unwrap() {
        if batch.tokens.iter().any(|t| t.kind == TokenKind::String) {
            found = true;
        }
        if batch.finished {
            break;
        }
    }
    assert!(found, "Did not find string token in streaming split scenario");
}

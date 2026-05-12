use rustine::parser::lexer::TokenKind;
use rustine::stream::{ChunkReader, StreamingLexer};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn write_temp(contents: &str, chunk: usize) -> (PathBuf, usize) {
    let path = std::env::temp_dir().join("gel_stream_multiline.gel");
    let mut f = File::create(&path).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    (path, chunk)
}

fn collect_tokens(path: &PathBuf, chunk: usize) -> Vec<rustine::stream::BorrowedToken> {
    let reader = ChunkReader::open(path, chunk).unwrap();
    let mut lex = StreamingLexer::new(reader);
    let mut out = Vec::new();
    while let Some(batch) = lex.next_batch().unwrap() {
        out.extend(batch.tokens);
        if batch.finished {
            break;
        }
    }
    out
}

#[test]
fn multiline_string_across_newline_and_chunks() {
    // A string that spans multiple lines and is split across chunk boundaries.
    // "Hello\nWorld" style with embedded newline. We force splits around the quote and newline.
    let source = "define ws /\\s+/\nmessage \"Hello\nWorld from Gelatin\"\n";
    let (path, chunk) = write_temp(source, 7); // small chunk size to force fragmentation
    let toks = collect_tokens(&path, chunk);
    assert!(
        toks.iter().any(|t| t.kind == TokenKind::String && t.len >= 20),
        "multiline string token missing or too small"
    );
}

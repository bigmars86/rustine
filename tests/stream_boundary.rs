use rustine::stream::{ChunkReader, StreamingLexer};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn write_temp(contents: &str, chunk: usize) -> (PathBuf, usize) {
    let id = FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("gel_stream_boundary_{}.gel", id));
    let mut f = File::create(&path).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    (path, chunk)
}

fn collect_tokens(path: &PathBuf, chunk: usize) -> Vec<rustine::stream::BorrowedToken> {
    let reader = ChunkReader::open(path, chunk).unwrap();
    let mut lex = StreamingLexer::new(reader);
    let mut out = Vec::new();
    while let Some(batch) = lex.next_batch().unwrap() {
        out.extend(batch.tokens.into_iter());
        if batch.finished {
            break;
        }
    }
    out
}

#[test]
fn regex_split_across_chunks() {
    // Force a regex literal to split: /[0-9]+/ broken in the middle
    let source = "define ws /\\s+/\n".to_string() + "define num /[0-" + "9]+/\n" + "grammar g:\n";
    let (path, chunk) = write_temp(&source, 8); // very small chunks
    let toks = collect_tokens(&path, chunk);
    assert!(
        toks.iter().any(|t| t.kind == rustine::parser::lexer::TokenKind::Regex),
        "regex token missing"
    );
}

#[test]
fn string_literal_split() {
    // Split a string literal across chunks
    let source = "define ws /\\s+/\nname 'Hello".to_string() + " World'\n";
    let (path, chunk) = write_temp(&source, 5);
    let toks = collect_tokens(&path, chunk);
    if !toks.iter().any(|t| t.kind == rustine::parser::lexer::TokenKind::String) {
        panic!(
            "string token missing; tokens: {:?}",
            toks.iter().map(|t| (t.kind, t.len)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn eof_token_emitted() {
    let source = "define ws /\\s+/\n";
    let (path, chunk) = write_temp(source, 16);
    let reader = ChunkReader::open(&path, chunk).unwrap();
    let mut lex = StreamingLexer::new(reader);
    let mut eof_seen = false;
    while let Some(batch) = lex.next_batch().unwrap() {
        if batch
            .tokens
            .iter()
            .any(|t| t.kind == rustine::parser::lexer::TokenKind::EOF)
        {
            eof_seen = true;
        }
        if batch.finished {
            break;
        }
    }
    assert!(eof_seen, "EOF not emitted");
}

use rustine::stream::{ChunkReader, StreamingLexer};
use std::env;
use std::fs::File;
use std::io::Write;

#[test]
fn test_streaming_basic() {
    // Create a temporary synthetic file (~50KB)
    let tmp_path = env::temp_dir().join("gel_stream_test.gel");
    let mut f = File::create(&tmp_path).unwrap();
    for _ in 0..1000 {
        // 1000 * ~50 chars ≈ 50KB
        writeln!(f, "define ws /\\s+/\ndefine num /[0-9]+/\ngrammar g:").unwrap();
    }

    let reader = ChunkReader::open(&tmp_path, 8192).unwrap();
    let mut lexer = StreamingLexer::new(reader);

    let mut total_tokens = 0usize;
    while let Some(batch) = lexer.next_batch().unwrap() {
        total_tokens += batch.tokens.len();
        if batch.finished {
            break;
        }
        if total_tokens > 10_000 {
            break;
        }
    }

    assert!(total_tokens > 0);
}

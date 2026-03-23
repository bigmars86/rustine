use crate::errors::{GelError, Span};
use crate::parser::lexer::{next_token, TokenKind};
use crate::stream::chunk_reader::ReadChunks;

pub struct StreamTokenBatch<'a> {
    pub tokens: Vec<BorrowedToken>,
    pub finished: bool,
    pub buffer: &'a str,
}

#[derive(Debug, Clone)]
pub struct BorrowedToken {
    pub kind: TokenKind,
    pub start: usize,
    pub len: usize,
    pub position: usize,
}

pub struct StreamingLexer<R: ReadChunks> {
    reader: R,
    pending: String,      // not yet fully tokenized bytes
    eof: bool,            // reader reported EOF
    global_offset: usize, // total consumed offset
    finished: bool,       // EOF token already emitted
    full_buffer: String,  // full emitted text (temporary until true zero-copy path)
}

impl<R: ReadChunks> StreamingLexer<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            pending: String::new(),
            eof: false,
            global_offset: 0,
            finished: false,
            full_buffer: String::new(),
        }
    }

    /// Fill pending buffer with next chunk if available.
    fn fill(&mut self) -> Result<(), GelError> {
        if self.eof {
            return Ok(());
        }
        if let Some(bytes) = self.reader.next_chunk().map_err(GelError::Io)? {
            let s = std::str::from_utf8(&bytes)
                .map_err(|e| GelError::lex(e.to_string(), Span::new(self.global_offset, 0, 0, 0)))?;
            self.pending.push_str(s);
        } else {
            self.eof = true;
        }
        Ok(())
    }

    /// Detect if rest likely represents a fragmented literal (string or regex without closing delimiter yet).
    fn looks_fragmented(rest: &str) -> bool {
        if rest.is_empty() {
            return false;
        }
        // Scan for unescaped closing delimiter.
        let starts = rest.chars().next().unwrap();
        if starts == '/' || starts == '"' || starts == '\'' {
            let mut escaped = false;
            for (_, c) in rest.char_indices().skip(1) {
                // skip opening
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == starts {
                    return false;
                } // found closing
            }
            return true; // no closing found
        }
        false
    }

    pub fn next_batch(&mut self) -> Result<Option<StreamTokenBatch<'_>>, GelError> {
        if self.finished {
            return Ok(None);
        }

        // If no pending data: attempt to fill
        if self.pending.is_empty() {
            self.fill()?;
        }
        if self.pending.is_empty() && self.eof {
            // Empty input → directly emit EOF
            self.finished = true;
            return Ok(Some(StreamTokenBatch {
                tokens: vec![BorrowedToken {
                    kind: TokenKind::EOF,
                    start: self.global_offset,
                    len: 0,
                    position: self.global_offset,
                }],
                finished: true,
                buffer: &self.full_buffer,
            }));
        }

        let mut batch = Vec::<BorrowedToken>::new();
        let mut local_consumed = 0usize; // bytes consumed this round
        let mut slice = self.pending.as_str();

        while !slice.is_empty() {
            match next_token(slice) {
                Ok((rest, tok)) => {
                    let consumed = slice.len() - rest.len();
                    // Filter comments / intra-line whitespace (same rules as full lexer)
                    let push_it = if tok.kind == TokenKind::Comment {
                        false
                    } else {
                        tok.kind != TokenKind::Newline || tok.slice.contains('\n')
                    };
                    if push_it {
                        let abs_start = self.global_offset + local_consumed;
                        batch.push(BorrowedToken {
                            kind: tok.kind,
                            start: abs_start,
                            len: tok.slice.len(),
                            position: abs_start,
                        });
                    }
                    // (debug logging removed for benchmarks)
                    slice = rest;
                    local_consumed += consumed;
                    // Limit batch size heuristically
                    if batch.len() >= 2048 {
                        break;
                    }
                }
                Err(_) => {
                    // No complete token recognized.
                    if self.eof {
                        if Self::looks_fragmented(slice) {
                            return Err(GelError::lex(
                                "Unterminated literal at EOF",
                                Span::new(self.global_offset + local_consumed, 0, 0, 0),
                            ));
                        } else {
                            // Consume one byte as garbage to progress instead of hard error.
                            let abs_start = self.global_offset + local_consumed;
                            let ch_len = slice.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                            batch.push(BorrowedToken {
                                kind: TokenKind::Comment,
                                start: abs_start,
                                len: 0,
                                position: abs_start,
                            }); // represent as skipped token (comment kind ignored downstream)
                            slice = &slice[ch_len..];
                            local_consumed += ch_len;
                        }
                    } else if Self::looks_fragmented(slice) {
                        // Wait for more bytes – treat as fragment, stop producing further tokens now.
                        break;
                    } else {
                        // Consume one byte as unknown to advance without failing.
                        let ch_len = slice.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                        local_consumed += ch_len;
                        slice = &slice[ch_len..];
                        // Do not push a token; continue scanning.
                    }
                }
            }
        }

        // Remove consumed bytes from pending
        if local_consumed > 0 {
            let consumed_segment = self.pending[..local_consumed].to_string();
            self.pending.replace_range(0..local_consumed, "");
            self.full_buffer.push_str(&consumed_segment);
            self.global_offset += local_consumed;
        }

        // If nothing produced and not EOF – fill and retry
        if batch.is_empty() && !self.eof {
            self.fill()?;
            if self.pending.is_empty() {
                return Ok(Some(StreamTokenBatch {
                    tokens: Vec::new(),
                    finished: false,
                    buffer: &self.full_buffer,
                }));
            }
            return self.next_batch(); // tail recursion (klein, bounded)
        }

        // EOF reached & pending empty → append EOF token (once)
        let finished;
        if self.eof && self.pending.is_empty() {
            self.finished = true;
            batch.push(BorrowedToken {
                kind: TokenKind::EOF,
                start: self.global_offset,
                len: 0,
                position: self.global_offset,
            });
            finished = true;
        } else {
            finished = false;
        }

        Ok(Some(StreamTokenBatch {
            tokens: batch,
            finished,
            buffer: &self.full_buffer,
        }))
    }
}

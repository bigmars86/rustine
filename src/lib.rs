//! Rustine — high-performance Gel syntax parser with PyO3 bridge
//! A Rust rewrite of the Python Gelatin library.

// On Linux builds with the `jemalloc` feature, use jemalloc for
// dramatically lower RSS after compact() frees build-time indices.
// jemalloc returns freed pages to the OS via madvise(MADV_DONTNEED),
// whereas glibc malloc keeps them mapped.
#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// On Windows (MSVC), use mimalloc for better memory return behavior.
// The default Windows heap allocator holds freed pages aggressively,
// inflating peak RSS by 30-50%.  mimalloc eagerly returns pages to
// the OS, giving RSS much closer to true live-set size.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_MI: mimalloc_dep::MiMalloc = mimalloc_dep::MiMalloc;

#[cfg(feature = "python")] // only expose bridge when python feature enabled
pub mod bridge;
pub mod exec; // existing execution module; submodules declared internally
pub mod parser;
pub mod stream;

#[cfg(feature = "python")]
use pyo3::prelude::*;

pub mod errors {
    use std::fmt;
    use thiserror::Error;

    /// Source location span attached to diagnostics and errors.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Span {
        /// Byte offset from start of input.
        pub offset: usize,
        /// 1-based line number.
        pub line: usize,
        /// 1-based column (characters, not bytes).
        pub col: usize,
        /// Length in bytes of the spanned region (0 = point span).
        pub len: usize,
    }

    impl fmt::Display for Span {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.line == 0 {
                write!(f, "offset {}", self.offset)
            } else {
                write!(f, "line {} col {}", self.line, self.col)
            }
        }
    }

    impl Span {
        pub fn new(offset: usize, line: usize, col: usize, len: usize) -> Self {
            Self { offset, line, col, len }
        }
        /// A zero/unknown span.
        pub fn unknown() -> Self {
            Self::default()
        }
    }

    /// Severity level for diagnostics emitted during parsing or execution.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Severity {
        Error,
        Warning,
        Info,
    }

    impl fmt::Display for Severity {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Severity::Error => write!(f, "error"),
                Severity::Warning => write!(f, "warning"),
                Severity::Info => write!(f, "info"),
            }
        }
    }

    /// A single diagnostic message with optional span and severity.
    #[derive(Debug, Clone)]
    pub struct Diagnostic {
        pub severity: Severity,
        pub message: String,
        pub span: Option<Span>,
    }

    impl fmt::Display for Diagnostic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.severity)?;
            if let Some(s) = &self.span {
                write!(f, " at {}", s)?;
            }
            write!(f, ": {}", self.message)
        }
    }

    impl Diagnostic {
        pub fn error(msg: impl Into<String>, span: Option<Span>) -> Self {
            Self {
                severity: Severity::Error,
                message: msg.into(),
                span,
            }
        }
        pub fn warning(msg: impl Into<String>, span: Option<Span>) -> Self {
            Self {
                severity: Severity::Warning,
                message: msg.into(),
                span,
            }
        }
        pub fn info(msg: impl Into<String>, span: Option<Span>) -> Self {
            Self {
                severity: Severity::Info,
                message: msg.into(),
                span,
            }
        }
    }

    #[derive(Debug, Error)]
    pub enum GelError {
        /// Lexer error: unexpected character or unterminated literal.
        #[error("{span}: lex error: {message}")]
        Lex { message: String, span: Span },

        /// Parser error: unexpected token or missing construct.
        #[error("{span}: parse error: {message}")]
        Parse { message: String, span: Span },

        /// Runtime / execution error (grammar not found, regex failure, etc.).
        #[error("{}: runtime error: {message}", span.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into()))]
        Runtime { message: String, span: Option<Span> },

        /// Semantic validation error (duplicate grammar, undefined reference, etc.).
        #[error("{span}: validation error: {message}")]
        Validation { message: String, span: Span },

        /// IO error (streaming, file reading).
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
    }

    impl GelError {
        /// Shorthand constructors.
        pub fn lex(msg: impl Into<String>, span: Span) -> Self {
            Self::Lex {
                message: msg.into(),
                span,
            }
        }
        pub fn parse(msg: impl Into<String>, span: Span) -> Self {
            Self::Parse {
                message: msg.into(),
                span,
            }
        }
        pub fn runtime(msg: impl Into<String>, span: Option<Span>) -> Self {
            Self::Runtime {
                message: msg.into(),
                span,
            }
        }
        pub fn validation(msg: impl Into<String>, span: Span) -> Self {
            Self::Validation {
                message: msg.into(),
                span,
            }
        }
        /// Extract the primary span if any.
        pub fn span(&self) -> Option<Span> {
            match self {
                Self::Lex { span, .. } | Self::Parse { span, .. } | Self::Validation { span, .. } => Some(*span),
                Self::Runtime { span, .. } => *span,
                _ => None,
            }
        }
    }

    pub type Result<T> = std::result::Result<T, GelError>;
}

pub use errors::{Diagnostic, GelError, Result, Severity, Span};
pub use exec::streaming::{StreamingEvent, StreamingRunner};
pub use exec::{execute_precompiled, serialize_execution, serialize_tree, serialize_tree_to_writer, RuntimeFormat};
pub use parser::{OutputFormat, Parser};

/// Convenience helper for tests and external callers to parse source and run a grammar against input.
pub fn parse_and_run(source: &str, grammar: &str, input: &str) -> Result<String> {
    let parser = Parser::new(OutputFormat::Json);
    parser.parse_and_run(source, grammar, input)
}

/// Python module export (only when feature "python" is active)
#[cfg(feature = "python")]
#[pymodule]
fn rustine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use bridge::{
        compile_file, compile_grammar, compile_string, parse_to_json, parse_to_xml, parse_to_yaml, run_grammar, run_grammar_xml,
        run_grammar_yaml, GelContext,
    };
    m.add_class::<GelContext>()?;
    m.add_function(wrap_pyfunction!(compile_grammar, m)?)?;
    m.add_function(wrap_pyfunction!(compile_file, m)?)?;
    m.add_function(wrap_pyfunction!(parse_to_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_to_xml, m)?)?;
    m.add_function(wrap_pyfunction!(parse_to_yaml, m)?)?;
    m.add_function(wrap_pyfunction!(run_grammar, m)?)?;
    m.add_function(wrap_pyfunction!(run_grammar_xml, m)?)?;
    m.add_function(wrap_pyfunction!(run_grammar_yaml, m)?)?;
    // Alias: compile_string matches Python Gelatin's Gelatin.util.compile_string
    m.add_function(wrap_pyfunction!(compile_string, m)?)?;
    // Version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

/// Execute a grammar from an already-parsed [`GelDocument`](crate::parser::ast::GelDocument)
/// against `input` and serialize the **data tree** in the chosen [`RuntimeFormat`].
///
/// This produces Python-compatible output (bare tree, no metadata envelope).
pub fn execute_and_serialize(
    doc: &mut crate::parser::ast::GelDocument,
    grammar: &str,
    input: &str,
    format: RuntimeFormat,
) -> Result<String> {
    let exec = exec::execute(doc, grammar, input)?;
    Ok(serialize_tree(&exec, format))
}

#[cfg(test)]
mod tests {
    use crate::parser::{OutputFormat, Parser};

    #[test]
    fn test_parser_creation() {
        let parser = Parser::new(OutputFormat::Json);
        // Use valid Gel syntax for testing
        let result = parser.parse_str("define ws /\\s+/");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_type() {
        use crate::errors::{GelError, Span};
        let error = GelError::lex("test error", Span::new(0, 1, 1, 0));
        let msg = format!("{}", error);
        assert!(msg.contains("test error"), "error display: {msg}");
        assert!(msg.contains("line 1"), "should show line: {msg}");
    }

    #[test]
    #[cfg(feature = "python")]
    fn test_native_json_parsing() {
        let result = crate::bridge::parse_gel_to_json("define ws /\\s+/");
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("defines"));
        assert!(json.contains("ws"));
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_native_xml_parsing() {
        let result = crate::bridge::parse_gel_to_xml("define ws /\\s+/");
        assert!(result.is_ok());
        let xml = result.unwrap();
        assert!(xml.contains("<gel-document>"));
        assert!(xml.contains("<define"));
    }
}

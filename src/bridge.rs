use crate::exec::{execute, execute_precompiled, serialize_tree, serialize_tree_to_writer, RuntimeFormat};
use crate::parser::ast::GelDocument;
use crate::parser::lexer::lex;
use crate::parser::syntax::parse_gel_document;
use crate::parser::validate;
use crate::parser::{OutputFormat, Parser};
use crate::{GelError, Result, Severity};
use pyo3::prelude::*;
use std::fs;

/// Map a GelError into an appropriate Python exception type.
fn gel_to_pyerr(e: GelError) -> PyErr {
    match &e {
        GelError::Lex { .. } | GelError::Parse { .. } | GelError::Validation { .. } => {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{e}"))
        }
        GelError::Runtime { .. } => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e}")),
        GelError::Io(_) => PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{e}")),
    }
}

/// Parse a format string into a RuntimeFormat, returning a PyErr on unknown values.
fn parse_format(format: &str) -> PyResult<RuntimeFormat> {
    match format {
        "json" => Ok(RuntimeFormat::Json),
        "xml" => Ok(RuntimeFormat::Xml),
        "yaml" | "yml" => Ok(RuntimeFormat::Yaml),
        "none" => Ok(RuntimeFormat::None),
        other => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown format '{}' (expected json, xml, yaml, none)",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// Compile-once / run-many context
// ---------------------------------------------------------------------------

/// Pre-compiled Gelatin grammar context.
///
/// Compile a `.gel` grammar once, then execute it against many inputs without
/// re-parsing.  This mirrors the Python ``Gelatin.util.compile()`` /
/// ``context.parse_string()`` workflow.
///
/// Example (Python):
///
/// ```python
/// import gelatin
/// ctx = gelatin.GelContext(open("syntax.gel").read())
/// print(ctx.run("input data here\n"))          # JSON
/// print(ctx.run_xml("input data here\n"))      # XML
/// print(ctx.run_yaml("input data here\n"))     # YAML
/// print(ctx.run("other input\n", grammar="other_grammar"))
/// ```
#[pyclass]
pub struct GelContext {
    doc: GelDocument,
}

#[pymethods]
impl GelContext {
    /// Compile a Gelatin grammar from source text.
    ///
    /// Raises ``ValueError`` on lex / parse / validation errors.
    #[new]
    fn new(gel_source: &str) -> PyResult<Self> {
        let tokens = lex(gel_source).map_err(gel_to_pyerr)?;
        let doc = parse_gel_document(&tokens).map_err(gel_to_pyerr)?;
        // Validate — fail on errors, print warnings
        let diags = validate::validate(&doc);
        for d in &diags {
            if d.severity == Severity::Error {
                return Err(gel_to_pyerr(GelError::validation(
                    d.message.clone(),
                    d.span.unwrap_or_default(),
                )));
            }
        }
        // Pre-compile all regexes so execute() won't need to mutate the document.
        let mut doc = doc;
        doc.compile_regexes();
        Ok(Self { doc })
    }

    /// Execute the grammar against `input` and return a JSON string.
    ///
    /// Parameters
    /// ----------
    /// input : str
    ///     The text to parse.
    /// grammar : str, optional
    ///     Name of the grammar entry-point (default ``"input"``).
    #[pyo3(signature = (input, grammar = "input"))]
    fn run(&self, input: &str, grammar: &str) -> PyResult<String> {
        let exec = execute_precompiled(&self.doc, grammar, input).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        Ok(serialize_tree(&exec, RuntimeFormat::Json))
    }

    /// Execute the grammar against `input` and return an XML string.
    #[pyo3(signature = (input, grammar = "input"))]
    fn run_xml(&self, input: &str, grammar: &str) -> PyResult<String> {
        let exec = execute_precompiled(&self.doc, grammar, input).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        Ok(serialize_tree(&exec, RuntimeFormat::Xml))
    }

    /// Execute the grammar against `input` and return a YAML string.
    #[pyo3(signature = (input, grammar = "input"))]
    fn run_yaml(&self, input: &str, grammar: &str) -> PyResult<String> {
        let exec = execute_precompiled(&self.doc, grammar, input).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        Ok(serialize_tree(&exec, RuntimeFormat::Yaml))
    }

    /// Execute and return in the requested format (``"json"``, ``"xml"``, ``"yaml"``).
    #[pyo3(signature = (input, format = "json", grammar = "input"))]
    fn generate_string(&self, input: &str, format: &str, grammar: &str) -> PyResult<String> {
        let fmt = parse_format(format)?;
        let exec = execute_precompiled(&self.doc, grammar, input).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        Ok(serialize_tree(&exec, fmt))
    }

    // ----- File I/O helpers (mirrors Python Gelatin util.*) -----

    /// Read an input file, execute the grammar, and return the output string.
    ///
    /// Mirrors ``Gelatin.util.generate(converter, input_file, format)``.
    ///
    /// Parameters
    /// ----------
    /// input_file : str
    ///     Path to the input file to convert.
    /// format : str, optional
    ///     Output format (``"json"``, ``"xml"``, ``"yaml"``). Default ``"json"``.
    /// grammar : str, optional
    ///     Entry-point grammar name. Default ``"input"``.
    /// encoding : str, optional
    ///     Input file encoding. Default ``"utf-8"`` (only UTF-8 supported natively).
    #[pyo3(signature = (input_file, format = "json", grammar = "input", encoding = "utf-8"))]
    fn generate(&self, input_file: &str, format: &str, grammar: &str, encoding: &str) -> PyResult<String> {
        let _ = encoding; // Rust reads as UTF-8; noted for API compatibility
        let content = fs::read_to_string(input_file)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", input_file, e)))?;
        self.generate_string(&content, format, grammar)
    }

    /// Read an input file, execute the grammar, and write the output to a file.
    ///
    /// Uses streaming output to avoid building the full result string in memory.
    /// Mirrors ``Gelatin.util.generate_to_file(converter, input_file, output_file, format)``.
    #[pyo3(signature = (input_file, output_file, format = "json", grammar = "input"))]
    fn generate_to_file(&self, input_file: &str, output_file: &str, format: &str, grammar: &str) -> PyResult<()> {
        let fmt = parse_format(format)?;
        let content = fs::read_to_string(input_file)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", input_file, e)))?;
        let exec = execute_precompiled(&self.doc, grammar, &content).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        let file = fs::File::create(output_file)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", output_file, e)))?;
        let mut writer = std::io::BufWriter::new(file);
        serialize_tree_to_writer(&exec, fmt, &mut writer)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", output_file, e)))?;
        Ok(())
    }

    /// Execute against a string input and write the output to a file.
    ///
    /// Uses streaming output to avoid building the full result string in memory.
    /// Mirrors ``Gelatin.util.generate_string_to_file(converter, input, output_file, format)``.
    #[pyo3(signature = (input, output_file, format = "json", grammar = "input"))]
    fn generate_string_to_file(&self, input: &str, output_file: &str, format: &str, grammar: &str) -> PyResult<()> {
        let fmt = parse_format(format)?;
        let exec = execute_precompiled(&self.doc, grammar, input).map_err(gel_to_pyerr)?;
        if let Some(err) = &exec.error {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
        }
        let file = fs::File::create(output_file)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", output_file, e)))?;
        let mut writer = std::io::BufWriter::new(file);
        serialize_tree_to_writer(&exec, fmt, &mut writer)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", output_file, e)))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Convenience free functions (compile + run in one shot)
// ---------------------------------------------------------------------------

/// Compile a Gelatin grammar from source text and return a reusable
/// [`GelContext`].  Equivalent to ``GelContext(gel_source)``.
#[pyfunction]
pub fn compile_grammar(gel_source: &str) -> PyResult<GelContext> {
    GelContext::new(gel_source)
}

/// Alias for [`compile_grammar`] — matches Python Gelatin's
/// ``Gelatin.util.compile_string(syntax)`` name.
#[pyfunction]
pub fn compile_string(gel_source: &str) -> PyResult<GelContext> {
    GelContext::new(gel_source)
}

/// Compile a Gelatin grammar from a ``.gel`` file.
///
/// Mirrors ``Gelatin.util.compile(syntax_file)``.
#[pyfunction]
#[pyo3(signature = (syntax_file, encoding = "utf-8"))]
pub fn compile_file(syntax_file: &str, encoding: &str) -> PyResult<GelContext> {
    let _ = encoding; // Rust reads as UTF-8
    let source = fs::read_to_string(syntax_file)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}: {}", syntax_file, e)))?;
    GelContext::new(&source)
}

/// Native Rust function to parse Gel-Syntax to JSON
pub fn parse_gel_to_json(input: &str) -> Result<String> {
    let parser = Parser::new(OutputFormat::Json);
    parser.parse_str(input)
}

/// Native Rust function to parse Gel-Syntax to XML
pub fn parse_gel_to_xml(input: &str) -> Result<String> {
    let parser = Parser::new(OutputFormat::Xml);
    parser.parse_str(input)
}

/// Native Rust function to parse Gel-Syntax to YAML
pub fn parse_gel_to_yaml(input: &str) -> Result<String> {
    let parser = Parser::new(OutputFormat::Yaml);
    parser.parse_str(input)
}

/// PyO3 wrapper for JSON parsing
#[pyfunction]
pub fn parse_to_json(input: &str) -> PyResult<String> {
    match parse_gel_to_json(input) {
        Ok(result) => Ok(result),
        Err(e) => Ok(format!("{{\"error\":\"{e}\"}}")),
    }
}

/// PyO3 wrapper for XML parsing
#[pyfunction]
pub fn parse_to_xml(input: &str) -> PyResult<String> {
    match parse_gel_to_xml(input) {
        Ok(result) => Ok(result),
        Err(e) => Ok(format!("<error>{e}</error>")),
    }
}

/// PyO3 wrapper for YAML parsing
#[pyfunction]
pub fn parse_to_yaml(input: &str) -> PyResult<String> {
    match parse_gel_to_yaml(input) {
        Ok(result) => Ok(result),
        Err(e) => Ok(format!("error: {e}")), // simple YAML-ish error scalar
    }
}

/// Execute a grammar against runtime input, returning JSON or raising on fail.
#[pyfunction]
pub fn run_grammar(gel_source: &str, grammar: &str, runtime_input: &str) -> PyResult<String> {
    let tokens = lex(gel_source).map_err(gel_to_pyerr)?;
    let mut doc = parse_gel_document(&tokens).map_err(gel_to_pyerr)?;
    let exec = execute(&mut doc, grammar, runtime_input).map_err(gel_to_pyerr)?;
    if let Some(err) = exec.error {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err));
    }
    Ok(serialize_tree(&exec, RuntimeFormat::Json))
}

/// Execute a grammar and return XML serialized runtime result (raises on fail).
#[pyfunction]
pub fn run_grammar_xml(gel_source: &str, grammar: &str, runtime_input: &str) -> PyResult<String> {
    let tokens = lex(gel_source).map_err(gel_to_pyerr)?;
    let mut doc = parse_gel_document(&tokens).map_err(gel_to_pyerr)?;
    let exec = execute(&mut doc, grammar, runtime_input).map_err(gel_to_pyerr)?;
    if let Some(err) = &exec.error {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
    }
    Ok(serialize_tree(&exec, RuntimeFormat::Xml))
}

/// Execute a grammar and return YAML serialized runtime result (raises on fail).
#[pyfunction]
pub fn run_grammar_yaml(gel_source: &str, grammar: &str, runtime_input: &str) -> PyResult<String> {
    let tokens = lex(gel_source).map_err(gel_to_pyerr)?;
    let mut doc = parse_gel_document(&tokens).map_err(gel_to_pyerr)?;
    let exec = execute(&mut doc, grammar, runtime_input).map_err(gel_to_pyerr)?;
    if let Some(err) = &exec.error {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(err.clone()));
    }
    Ok(serialize_tree(&exec, RuntimeFormat::Yaml))
}

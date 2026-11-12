use crate::exec::{execute_tree, serialize_tree, serialize_tree_to_writer, ExecutionResult, RuntimeFormat};
use crate::parser::ast::GelDocument;
use crate::parser::lexer::lex;
use crate::parser::syntax::parse_gel_document;
use crate::parser::validate;
use crate::parser::{OutputFormat, Parser};
use crate::{GelError, Result, Severity};
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::fs;
use std::io::Write;

/// Map a GelError into an appropriate Python exception type.
fn gel_to_pyerr(e: GelError) -> PyErr {
    match &e {
        GelError::Lex { .. } | GelError::Parse { .. } | GelError::Validation { .. } => {
            PyValueError::new_err(e.to_string())
        }
        GelError::Runtime { .. } => PyRuntimeError::new_err(e.to_string()),
        GelError::Io(_) => PyIOError::new_err(e.to_string()),
    }
}

/// The execution result; grammar errors and `do.fail` become Python exceptions.
fn checked(result: Result<ExecutionResult>) -> PyResult<ExecutionResult> {
    let exec = result.map_err(gel_to_pyerr)?;
    match &exec.error {
        Some(err) => Err(PyRuntimeError::new_err(err.clone())),
        None => Ok(exec),
    }
}

fn parse_doc(gel_source: &str, preserve_case: bool) -> PyResult<GelDocument> {
    let tokens = lex(gel_source).map_err(gel_to_pyerr)?;
    let mut doc = parse_gel_document(&tokens).map_err(gel_to_pyerr)?;
    doc.preserve_case = preserve_case;
    Ok(doc)
}

fn read_file(path: &str) -> PyResult<String> {
    fs::read_to_string(path).map_err(|e| PyIOError::new_err(format!("{}: {}", path, e)))
}

/// Files are always UTF-8; any other `encoding` raises ``ValueError`` instead of being ignored.
fn require_utf8(py: Python<'_>, encoding: &str) -> PyResult<()> {
    let codec = py.import("codecs")?.call_method1("lookup", (encoding,))?;
    if codec.getattr("name")?.extract::<String>()? == "utf-8" {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "only UTF-8 is supported, not '{encoding}'"
        )))
    }
}

fn write_tree(exec: &ExecutionResult, fmt: RuntimeFormat, path: &str) -> PyResult<()> {
    let io_err = |e: std::io::Error| PyIOError::new_err(format!("{}: {}", path, e));
    let mut writer = std::io::BufWriter::new(fs::File::create(path).map_err(io_err)?);
    serialize_tree_to_writer(exec, fmt, &mut writer).map_err(io_err)?;
    writer.flush().map_err(io_err)
}

/// Parse a format string into a RuntimeFormat, returning a PyErr on unknown values.
fn parse_format(format: &str) -> PyResult<RuntimeFormat> {
    match format {
        "json" => Ok(RuntimeFormat::Json),
        "xml" => Ok(RuntimeFormat::Xml),
        "yaml" | "yml" => Ok(RuntimeFormat::Yaml),
        "none" => Ok(RuntimeFormat::None),
        other => Err(PyValueError::new_err(format!(
            "unknown format '{}' (expected json, xml, yaml, none)",
            other
        ))),
    }
}

/// A compiled grammar: compile once, run against many inputs.
///
/// ```python
/// from Rustine import rustine
/// ctx = rustine.GelContext(open("syntax.gel").read())
/// print(ctx.generate_string("input data\n", format="xml"))
/// ```
#[pyclass]
pub struct GelContext {
    doc: GelDocument,
}

#[pymethods]
impl GelContext {
    /// Compile a grammar from source text.
    ///
    /// ``preserve_case=True`` keeps the case of tag names and attribute keys in
    /// ``out.*`` paths instead of lowercasing them.
    ///
    /// Raises ``ValueError`` on lex / parse / validation errors.
    #[new]
    #[pyo3(signature = (gel_source, *, preserve_case = false))]
    fn new(gel_source: &str, preserve_case: bool) -> PyResult<Self> {
        let mut doc = parse_doc(gel_source, preserve_case)?;
        for d in validate::validate(&doc) {
            if d.severity == Severity::Error {
                return Err(gel_to_pyerr(GelError::validation(
                    d.message,
                    d.span.unwrap_or_default(),
                )));
            }
        }
        doc.compile_regexes(); // required by execute_tree
        Ok(Self { doc })
    }

    /// Run the grammar on `input` and return JSON.
    #[pyo3(signature = (input, grammar = "input"))]
    fn run(&self, py: Python<'_>, input: &str, grammar: &str) -> PyResult<String> {
        self.render(py, input, grammar, RuntimeFormat::Json)
    }

    /// Run the grammar on `input` and return XML.
    #[pyo3(signature = (input, grammar = "input"))]
    fn run_xml(&self, py: Python<'_>, input: &str, grammar: &str) -> PyResult<String> {
        self.render(py, input, grammar, RuntimeFormat::Xml)
    }

    /// Run the grammar on `input` and return YAML.
    #[pyo3(signature = (input, grammar = "input"))]
    fn run_yaml(&self, py: Python<'_>, input: &str, grammar: &str) -> PyResult<String> {
        self.render(py, input, grammar, RuntimeFormat::Yaml)
    }

    /// Run the grammar on `input`; ``format`` is ``"json"``, ``"xml"``, ``"yaml"`` or ``"none"``.
    #[pyo3(signature = (input, format = "json", grammar = "input"))]
    fn generate_string(&self, py: Python<'_>, input: &str, format: &str, grammar: &str) -> PyResult<String> {
        let fmt = parse_format(format)?;
        self.render(py, input, grammar, fmt)
    }

    /// Like ``generate_string``, reading the input from a UTF-8 file.
    #[pyo3(signature = (input_file, format = "json", grammar = "input", encoding = "utf-8"))]
    fn generate(
        &self,
        py: Python<'_>,
        input_file: &str,
        format: &str,
        grammar: &str,
        encoding: &str,
    ) -> PyResult<String> {
        require_utf8(py, encoding)?;
        let fmt = parse_format(format)?;
        self.render(py, &read_file(input_file)?, grammar, fmt)
    }

    /// Run the grammar on a file and stream the output to ``output_file``.
    #[pyo3(signature = (input_file, output_file, format = "json", grammar = "input"))]
    fn generate_to_file(
        &self,
        py: Python<'_>,
        input_file: &str,
        output_file: &str,
        format: &str,
        grammar: &str,
    ) -> PyResult<()> {
        let fmt = parse_format(format)?;
        let input = read_file(input_file)?;
        py.detach(|| write_tree(&self.exec_input(&input, grammar)?, fmt, output_file))
    }

    /// Run the grammar on `input` and stream the output to ``output_file``.
    #[pyo3(signature = (input, output_file, format = "json", grammar = "input"))]
    fn generate_string_to_file(
        &self,
        py: Python<'_>,
        input: &str,
        output_file: &str,
        format: &str,
        grammar: &str,
    ) -> PyResult<()> {
        let fmt = parse_format(format)?;
        py.detach(|| write_tree(&self.exec_input(input, grammar)?, fmt, output_file))
    }
}

impl GelContext {
    fn exec_input(&self, input: &str, grammar: &str) -> PyResult<ExecutionResult> {
        checked(execute_tree(&self.doc, grammar, input))
    }

    /// Run and serialize without holding the GIL, so other Python threads keep running.
    fn render(&self, py: Python<'_>, input: &str, grammar: &str, fmt: RuntimeFormat) -> PyResult<String> {
        py.detach(|| Ok(serialize_tree(&self.exec_input(input, grammar)?, fmt)))
    }
}

/// Compile a grammar from source text; same as ``GelContext(gel_source)``.
#[pyfunction]
#[pyo3(signature = (gel_source, *, preserve_case = false))]
pub fn compile_grammar(gel_source: &str, preserve_case: bool) -> PyResult<GelContext> {
    GelContext::new(gel_source, preserve_case)
}

/// Alias of [`compile_grammar`].
#[pyfunction]
#[pyo3(signature = (gel_source, *, preserve_case = false))]
pub fn compile_string(gel_source: &str, preserve_case: bool) -> PyResult<GelContext> {
    GelContext::new(gel_source, preserve_case)
}

/// Compile a grammar from a UTF-8 ``.gel`` file.
#[pyfunction]
#[pyo3(signature = (syntax_file, encoding = "utf-8", *, preserve_case = false))]
pub fn compile_file(py: Python<'_>, syntax_file: &str, encoding: &str, preserve_case: bool) -> PyResult<GelContext> {
    require_utf8(py, encoding)?;
    GelContext::new(&read_file(syntax_file)?, preserve_case)
}

/// Serialize the grammar's AST as JSON.
pub fn parse_gel_to_json(input: &str) -> Result<String> {
    let parser = Parser::new(OutputFormat::Json);
    parser.parse_str(input)
}

/// Return the grammar's AST as JSON; an invalid grammar raises ``ValueError``.
#[pyfunction]
pub fn parse_to_json(input: &str) -> PyResult<String> {
    parse_gel_to_json(input).map_err(gel_to_pyerr)
}

fn run_source(
    py: Python<'_>,
    gel_source: &str,
    grammar: &str,
    input: &str,
    preserve_case: bool,
    fmt: RuntimeFormat,
) -> PyResult<String> {
    GelContext::new(gel_source, preserve_case)?.render(py, input, grammar, fmt)
}

/// Compile `gel_source`, run `grammar` on `runtime_input` and return JSON.
#[pyfunction]
#[pyo3(signature = (gel_source, grammar, runtime_input, *, preserve_case = false))]
pub fn run_grammar(
    py: Python<'_>,
    gel_source: &str,
    grammar: &str,
    runtime_input: &str,
    preserve_case: bool,
) -> PyResult<String> {
    run_source(
        py,
        gel_source,
        grammar,
        runtime_input,
        preserve_case,
        RuntimeFormat::Json,
    )
}

/// Like [`run_grammar`], returning XML.
#[pyfunction]
#[pyo3(signature = (gel_source, grammar, runtime_input, *, preserve_case = false))]
pub fn run_grammar_xml(
    py: Python<'_>,
    gel_source: &str,
    grammar: &str,
    runtime_input: &str,
    preserve_case: bool,
) -> PyResult<String> {
    run_source(
        py,
        gel_source,
        grammar,
        runtime_input,
        preserve_case,
        RuntimeFormat::Xml,
    )
}

/// Like [`run_grammar`], returning YAML.
#[pyfunction]
#[pyo3(signature = (gel_source, grammar, runtime_input, *, preserve_case = false))]
pub fn run_grammar_yaml(
    py: Python<'_>,
    gel_source: &str,
    grammar: &str,
    runtime_input: &str,
    preserve_case: bool,
) -> PyResult<String> {
    run_source(
        py,
        gel_source,
        grammar,
        runtime_input,
        preserve_case,
        RuntimeFormat::Yaml,
    )
}

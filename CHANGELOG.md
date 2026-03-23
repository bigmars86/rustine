# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-23

### Added

- Complete Rust port of the Python Gelatin parser engine
- Gel grammar language: match, imatch, when, skip, define, grammar inheritance
- Output actions: create, add, replace, add_attribute, set_root_name, open,
  enter, leave
- Trigger system: enqueue_before/after/on_add/on_leave with single-shot and
  persistent variants
- Capture variables: positional ($1, $2) and named ($name) with interpolation
- Multi-format output: JSON, XML, YAML
- Streaming execution via StreamingRunner
- Structured error model with source spans (GelError)
- Semantic validation: regex pre-validation, inheritance checks, undefined
  grammar/variable warnings
- Python bindings via PyO3 + maturin
- Backward-compatible Gelatin Python shim
- CLI tool: rgel (-s, -f, -g options; compatible with gel)
- Zero-copy captures via Cow\<str\>
- Arena allocation via bumpalo
- O(1) child lookup via index maps
- Lazy regex compilation and caching
- 55 test suites with near-perfect parity vs Python Gelatin
- Criterion benchmarks: exec, fixtures, scale (25/50/100 MB), real-data
- Cross-platform benchmarks: Windows MSVC, Linux glibc, Linux jemalloc
- Native CRLF support: grammar patterns use `\\r?\\n` to handle both LF and
  CRLF line endings without input pre-processing

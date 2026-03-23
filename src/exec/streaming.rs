//! Streaming execution engine: processes input incrementally in chunks.
//!
//! While the batch [`Runner`](super::Runner) requires the full input up front,
//! `StreamingRunner` accumulates chunks and executes the grammar as far as
//! possible after each feed, yielding [`StreamingEvent`]s as output tree
//! mutations occur.
//!
//! # Example
//! ```ignore
//! let mut sr = StreamingRunner::new(&mut doc, "main")?;
//! for chunk in chunks {
//!     sr.feed(chunk);
//!     for event in sr.step()? {
//!         handle(event);
//!     }
//! }
//! let result = sr.finish()?;
//! ```

use std::sync::Arc;

use crate::errors::{Diagnostic, GelError, Result};
use crate::exec::out::{ActionExecutor, OutputTree, RuntimeAction};
use crate::exec::{ExecutionResult, TriggerAction};
use crate::parser::ast::{Expression, FunctionCall, GelDocument, Statement};
use regex::Regex;

// ── Streaming events ────────────────────────────────────────────────

/// An incremental output event emitted during streaming execution.
///
/// Each variant mirrors a [`RuntimeAction`] but owns its data so it can
/// be buffered, serialized, or sent across threads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingEvent {
    /// `out.create("path", value)` — create a node.
    Create { path: String, value: Option<String> },
    /// `out.add("path", value)` — append to or create a node.
    Add { path: String, value: Option<String> },
    /// `out.replace("path", value)` — replace node text.
    Replace { path: String, value: Option<String> },
    /// `out.add_attribute("path", "name", "value")`.
    AddAttribute { path: String, name: String, value: String },
    /// `out.set_root_name("name")`.
    SetRootName { name: String },
    /// `out.open("path")` — push a new scope.
    Open { path: String },
    /// `out.enter("path")` — enter an existing scope.
    Enter { path: String },
    /// `out.leave` — pop scope.
    Leave,
    /// A match consumed `len` bytes at absolute position `pos`.
    MatchConsumed { pos: usize, len: usize },
    /// A when block fired.
    WhenFired,
    /// Input fully consumed or no more progress possible.
    Stalled { pos: usize, remaining: usize },
    /// Execution finished (all input consumed or grammar ended).
    Finished { consumed: usize },
}

impl StreamingEvent {
    /// Convert a [`RuntimeAction`] to an owned [`StreamingEvent`].
    fn from_action(act: &RuntimeAction<'_>) -> Self {
        match act {
            RuntimeAction::OutCreate { path, value } => Self::Create {
                path: path.to_string(),
                value: value.map(|v| v.to_string()),
            },
            RuntimeAction::OutAdd { path, value } => Self::Add {
                path: path.to_string(),
                value: value.map(|v| v.to_string()),
            },
            RuntimeAction::OutReplace { path, value } => Self::Replace {
                path: path.to_string(),
                value: value.map(|v| v.to_string()),
            },
            RuntimeAction::OutAddAttribute { path, name, value } => Self::AddAttribute {
                path: path.to_string(),
                name: name.to_string(),
                value: value.to_string(),
            },
            RuntimeAction::OutSetRootName { name } => Self::SetRootName { name: name.to_string() },
            RuntimeAction::OutOpen { path } => Self::Open { path: path.to_string() },
            RuntimeAction::OutEnter { path } => Self::Enter { path: path.to_string() },
            RuntimeAction::OutLeave => Self::Leave,
        }
    }
}

// ── StreamingRunner ─────────────────────────────────────────────────

/// Incremental execution engine that processes input chunk by chunk.
///
/// Internally re-uses the same matching / trigger logic as the batch
/// [`Runner`](super::Runner), but operates on a growable input buffer
/// and tracks how far execution has progressed.
pub struct StreamingRunner {
    /// The parsed Gel document (grammar definitions + regex cache).
    doc: GelDocument,
    /// Accumulated input text (grows as chunks are fed).
    buffer: String,
    /// Current byte position within `buffer`.
    pos: usize,
    /// Whether the caller has signalled that no more input is coming.
    eof: bool,
    /// Name of the grammar being executed.
    grammar_name: String,
    /// Flattened & inherited statement list.
    statements: Vec<Statement>,
    /// Output tree (built incrementally).
    tree: OutputTree,
    /// Execution traces.
    traces: Vec<String>,
    /// Diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// Collected actions (for final result).
    actions: Vec<FunctionCall>,
    /// Capture history.
    capture_history: Vec<Vec<String>>,
    capture_names_history: Vec<Vec<Option<Arc<str>>>>,
    /// Error state.
    error: Option<String>,
    /// Total bytes consumed across all steps.
    consumed: usize,
    /// Triggers (single-shot).
    trig_before: Vec<(Regex, TriggerAction)>,
    trig_after: Vec<(Regex, TriggerAction)>,
    trig_on_add: Vec<(Regex, TriggerAction)>,
    trig_on_leave: Vec<(Regex, TriggerAction)>,
    /// Triggers (persistent).
    trig_before_persist: Vec<(Regex, TriggerAction)>,
    trig_after_persist: Vec<(Regex, TriggerAction)>,
    trig_on_add_persist: Vec<(Regex, TriggerAction)>,
    trig_on_leave_persist: Vec<(Regex, TriggerAction)>,
    /// Last match state.
    last_match_text: String,
    last_captures: Vec<String>,
    last_capture_names: Vec<Option<Arc<str>>>,
}

impl StreamingRunner {
    /// Create a new streaming runner for the given grammar.
    ///
    /// The `GelDocument` is cloned internally so the runner owns its data.
    pub fn new(doc: &mut GelDocument, grammar_name: &str) -> Result<Self> {
        if !doc.grammars.contains_key(grammar_name) {
            return Err(GelError::runtime(format!("Grammar not found: {}", grammar_name), None));
        }

        let statements = super::collect_inherited_statements(doc, grammar_name);

        Ok(Self {
            doc: doc.clone(),
            buffer: String::new(),
            pos: 0,
            eof: false,
            grammar_name: grammar_name.to_string(),
            statements,
            tree: OutputTree::new(),
            traces: Vec::new(),
            diagnostics: Vec::new(),
            actions: Vec::new(),
            capture_history: Vec::new(),
            capture_names_history: Vec::new(),
            error: None,
            consumed: 0,
            trig_before: Vec::new(),
            trig_after: Vec::new(),
            trig_on_add: Vec::new(),
            trig_on_leave: Vec::new(),
            trig_before_persist: Vec::new(),
            trig_after_persist: Vec::new(),
            trig_on_add_persist: Vec::new(),
            trig_on_leave_persist: Vec::new(),
            last_match_text: String::new(),
            last_captures: Vec::new(),
            last_capture_names: Vec::new(),
        })
    }

    /// Feed a chunk of input text into the buffer.
    pub fn feed(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }

    /// Signal that no more input will be provided.
    pub fn set_eof(&mut self) {
        self.eof = true;
    }

    /// Current byte position within the accumulated buffer.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Total bytes consumed so far.
    pub fn total_consumed(&self) -> usize {
        self.consumed
    }

    /// Reference to the output tree being built.
    pub fn output(&self) -> &OutputTree {
        &self.tree
    }

    /// Run grammar statements against available input until no more progress
    /// can be made or the grammar ends. Returns events produced during this step.
    pub fn step(&mut self) -> Result<Vec<StreamingEvent>> {
        let mut events = Vec::new();

        loop {
            let start = self.pos;

            if self.pos >= self.buffer.len() {
                // No more input currently available.
                if self.eof {
                    events.push(StreamingEvent::Finished {
                        consumed: self.consumed,
                    });
                } else {
                    events.push(StreamingEvent::Stalled {
                        pos: self.pos,
                        remaining: self.buffer.len() - self.pos,
                    });
                }
                break;
            }

            let mut matched_any = false;

            for stmt in &self.statements.clone() {
                match stmt {
                    Statement::Match(m) => {
                        let mut scope: Vec<String> = Vec::new();
                        let mut name_scope: Vec<Option<Arc<str>>> = Vec::new();
                        if let Some((len, acts)) = self.eval_match_streaming(m, &mut scope, &mut name_scope)? {
                            if len > 0 {
                                self.last_match_text = self.buffer[self.pos..self.pos + len].to_string();
                                self.pos += len;
                                self.consumed += len;
                                self.last_captures = scope.clone();
                                self.last_capture_names = name_scope.clone();

                                self.fire_triggers_streaming("before", &mut events);
                                let substituted = self.substitute_actions_streaming(&acts, &scope, &name_scope);
                                let (return_levels, did_next, fail_error, auto_leaves) =
                                    self.execute_runtime_actions_streaming(&substituted, &mut events);

                                for _ in 0..auto_leaves {
                                    self.fire_triggers_streaming("on_leave", &mut events);
                                    self.tree.leave();
                                    events.push(StreamingEvent::Leave);
                                }
                                if let Some(err) = fail_error {
                                    self.error = Some(err);
                                }
                                if return_levels > 0 {
                                    events.push(StreamingEvent::Finished {
                                        consumed: self.consumed,
                                    });
                                    return Ok(events);
                                }
                                if !scope.is_empty() {
                                    self.capture_history.push(scope.clone());
                                    self.capture_names_history.push(name_scope.clone());
                                }
                                self.actions.extend(substituted);
                                self.fire_triggers_streaming("after", &mut events);
                                events.push(StreamingEvent::MatchConsumed { pos: start, len });
                                self.traces.push(format!("match consumed {} chars", len));
                                matched_any = true;
                                if did_next {
                                    break;
                                }
                                break; // restart after successful match
                            }
                        }
                    }
                    Statement::When(w) => {
                        let mut scope: Vec<String> = Vec::new();
                        let mut name_scope: Vec<Option<Arc<str>>> = Vec::new();
                        if self.eval_when_streaming(w, &mut scope, &mut name_scope)? {
                            let substituted = self.substitute_actions_streaming(&w.actions, &scope, &name_scope);
                            self.last_match_text.clear();
                            self.last_captures = scope.clone();
                            self.last_capture_names = name_scope.clone();

                            self.fire_triggers_streaming("before", &mut events);
                            let (return_levels, did_next, fail_error, auto_leaves) =
                                self.execute_runtime_actions_streaming(&substituted, &mut events);

                            for _ in 0..auto_leaves {
                                self.fire_triggers_streaming("on_leave", &mut events);
                                self.tree.leave();
                                events.push(StreamingEvent::Leave);
                            }
                            if let Some(err) = fail_error {
                                self.error = Some(err);
                            }
                            if return_levels > 0 {
                                events.push(StreamingEvent::Finished {
                                    consumed: self.consumed,
                                });
                                return Ok(events);
                            }
                            if !scope.is_empty() {
                                self.capture_history.push(scope.clone());
                                self.capture_names_history.push(name_scope.clone());
                            }
                            self.actions.extend(substituted);
                            self.fire_triggers_streaming("after", &mut events);
                            events.push(StreamingEvent::WhenFired);
                            self.traces.push("when triggered".to_string());
                            matched_any = true;
                            if did_next {
                                break;
                            }
                            break; // restart after when
                        }
                    }
                    Statement::Skip(s) => {
                        if let Some(len) = self.eval_skip_streaming(s)? {
                            if len > 0 {
                                self.pos += len;
                                self.consumed += len;
                                self.traces.push(format!("skip consumed {} chars", len));
                                matched_any = true;
                                break;
                            }
                        }
                    }
                    Statement::Action(a) => {
                        let pos_before_action = self.pos;
                        let substituted = self.substitute_action_streaming(a, &[], &[]);
                        let (return_levels, did_next, fail_error, auto_leaves) =
                            self.execute_runtime_actions_streaming(std::slice::from_ref(&substituted), &mut events);
                        for _ in 0..auto_leaves {
                            self.fire_triggers_streaming("on_leave", &mut events);
                            self.tree.leave();
                            events.push(StreamingEvent::Leave);
                        }
                        if let Some(err) = fail_error {
                            self.error = Some(err);
                        }
                        if return_levels > 0 {
                            events.push(StreamingEvent::Finished {
                                consumed: self.consumed,
                            });
                            return Ok(events);
                        }
                        if did_next {
                            matched_any = true;
                            break;
                        }
                        // If a sub-grammar consumed input, break+restart
                        // (Python: Grammar.parse() restarts the statement loop on any success)
                        if self.pos > pos_before_action {
                            matched_any = true;
                            break;
                        }
                        self.actions.push(substituted);
                        self.traces.push(format!("action {} invoked", a.name));
                    }
                }
            }

            if !matched_any {
                // No statement matched — stalled or grammar ended.
                if self.eof {
                    self.traces.push(format!(
                        "no match found in grammar {}, pos {}",
                        self.grammar_name, self.pos
                    ));
                    events.push(StreamingEvent::Finished {
                        consumed: self.consumed,
                    });
                } else {
                    events.push(StreamingEvent::Stalled {
                        pos: self.pos,
                        remaining: self.buffer.len() - self.pos,
                    });
                }
                break;
            }
        }

        Ok(events)
    }

    /// Finalize the streaming execution and produce a complete [`ExecutionResult`].
    pub fn finish(mut self) -> Result<ExecutionResult> {
        self.eof = true;
        let _ = self.step()?; // drain remaining
        Ok(ExecutionResult {
            consumed: self.consumed,
            actions: self.actions,
            traces: self.traces,
            diagnostics: self.diagnostics,
            capture_history: self.capture_history,
            capture_names_history: self.capture_names_history,
            output: self.tree,
            flat: None,
            error: self.error,
        })
    }

    // ── Internal helpers (simplified streaming variants) ────────────

    fn eval_match_streaming(
        &mut self,
        m: &crate::parser::ast::MatchStatement,
        captures_out: &mut Vec<String>,
        names_out: &mut Vec<Option<Arc<str>>>,
    ) -> Result<Option<(usize, Vec<FunctionCall>)>> {
        for alt in &m.match_list.alternatives {
            if let Some((len, groups, names)) = self.eval_field_list_streaming(alt, m.case_insensitive)? {
                captures_out.extend(groups);
                names_out.extend(names);
                return Ok(Some((len, m.actions.clone())));
            }
        }
        Ok(None)
    }

    fn eval_when_streaming(
        &mut self,
        w: &crate::parser::ast::WhenStatement,
        captures_out: &mut Vec<String>,
        names_out: &mut Vec<Option<Arc<str>>>,
    ) -> Result<bool> {
        for alt in &w.match_list.alternatives {
            if let Some((_, groups, names)) = self.eval_field_list_streaming(alt, false)? {
                captures_out.extend(groups);
                names_out.extend(names);
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn eval_skip_streaming(&mut self, s: &crate::parser::ast::SkipStatement) -> Result<Option<usize>> {
        let pos = self.pos;
        let rem = &self.buffer[pos..];
        // Fast path: use memchr for common skip patterns to avoid regex overhead
        if let crate::parser::ast::Expression::Regex(r) = &s.pattern {
            if let Some(n) = crate::exec::Runner::try_fast_skip(r, rem) {
                return Ok(Some(n));
            }
        }
        // eval_expression_streaming may mutate self.doc (ensure_compiled)
        // but rem is borrowed from self.buffer — we need to clone the slice.
        let rem_owned = rem.to_string();
        self.eval_expression_streaming(&s.pattern, &rem_owned, false, &[], &[])
            .map(|o| o.map(|(n, _)| n))
    }

    #[allow(clippy::type_complexity)]
    fn eval_field_list_streaming(
        &mut self,
        list: &crate::parser::ast::MatchFieldList,
        case_insensitive: bool,
    ) -> Result<Option<(usize, Vec<String>, Vec<Option<Arc<str>>>)>> {
        let mut offset = 0usize;
        let pos = self.pos;
        // Clone the remaining buffer slice to avoid holding &self while calling &mut self.
        let buf_owned = self.buffer[pos..].to_string();
        let mut rem: &str = &buf_owned;
        let mut all_captures: Vec<String> = Vec::new();
        let mut all_names: Vec<Option<Arc<str>>> = Vec::new();
        for expr in &list.expressions {
            match self.eval_expression_streaming(expr, rem, case_insensitive, &all_captures, &all_names)? {
                Some((consumed, groups)) => {
                    if consumed > rem.len() {
                        return Ok(None);
                    }
                    offset += consumed;
                    rem = &rem[consumed..];
                    for (text, name) in groups {
                        all_captures.push(text);
                        all_names.push(name);
                    }
                }
                None => return Ok(None),
            }
        }
        Ok(Some((offset, all_captures, all_names)))
    }

    #[allow(clippy::type_complexity)]
    fn eval_expression_streaming(
        &mut self,
        expr: &Expression,
        rem: &str,
        case_insensitive: bool,
        captures_so_far: &[String],
        names_so_far: &[Option<Arc<str>>],
    ) -> Result<Option<(usize, Vec<(String, Option<Arc<str>>)>)>> {
        match expr {
            Expression::Regex(r) => {
                if let Err(e) = self.doc.ensure_compiled(r, case_insensitive) {
                    return Err(GelError::runtime(e, None));
                }
                let idx = *self
                    .doc
                    .pattern_indices
                    .get(r)
                    .ok_or_else(|| GelError::runtime(format!("Regex index not found: {}", r), None))?;
                let slot = idx * 2 + case_insensitive as usize;
                let compiled = self
                    .doc
                    .regex_cache
                    .get(slot)
                    .and_then(|o| o.as_ref())
                    .ok_or_else(|| GelError::runtime(format!("Regex missing after ensure: {}", r), None))?;
                if let Some(mat) = compiled.captures(rem) {
                    if mat.get(0).map(|m| m.start()).unwrap_or(1) == 0 {
                        let full = mat.get(0).unwrap();
                        let mut groups = Vec::new();
                        for i in 0..mat.len() {
                            let text = mat.get(i).map(|m| m.as_str()).unwrap_or("").to_string();
                            let name: Option<Arc<str>> = compiled.capture_names().nth(i).flatten().map(Arc::from);
                            groups.push((text, name));
                        }
                        return Ok(Some((full.end(), groups)));
                    }
                }
                Ok(None)
            }
            Expression::String(s) => {
                let compare = if case_insensitive { s.to_lowercase() } else { s.clone() };
                let check = if case_insensitive {
                    rem.to_lowercase()
                } else {
                    rem.to_string()
                };
                if check.starts_with(&compare) {
                    Ok(Some((s.len(), vec![(s.clone(), None)])))
                } else {
                    Ok(None)
                }
            }
            Expression::Number(n) => {
                let ns = n.to_string();
                if rem.starts_with(&ns) {
                    Ok(Some((ns.len(), vec![(ns, None)])))
                } else {
                    Ok(None)
                }
            }
            Expression::Capture(idx) => {
                let val = captures_so_far.get(*idx).cloned().unwrap_or_default();
                if !val.is_empty() && rem.starts_with(&val) {
                    Ok(Some((val.len(), vec![(val, None)])))
                } else {
                    Ok(None)
                }
            }
            Expression::CaptureName(name) => {
                let val = names_so_far
                    .iter()
                    .enumerate()
                    .find(|(_, n)| n.as_deref() == Some(name.as_str()))
                    .and_then(|(i, _)| captures_so_far.get(i))
                    .cloned()
                    .unwrap_or_default();
                if !val.is_empty() && rem.starts_with(&val) {
                    Ok(Some((val.len(), vec![(val, None)])))
                } else {
                    Ok(None)
                }
            }
            Expression::Variable(v) => {
                if let Some(inner) = self.resolve_variable_streaming(v) {
                    self.eval_expression_streaming(&inner, rem, case_insensitive, captures_so_far, names_so_far)
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn resolve_variable_streaming(&self, name: &str) -> Option<Expression> {
        self.doc.defines.get(name).cloned()
    }

    fn substitute_actions_streaming(
        &self,
        actions: &[FunctionCall],
        scope: &[String],
        name_scope: &[Option<Arc<str>>],
    ) -> Vec<FunctionCall> {
        actions
            .iter()
            .map(|a| self.substitute_action_streaming(a, scope, name_scope))
            .collect()
    }

    fn substitute_action_streaming(
        &self,
        action: &FunctionCall,
        scope: &[String],
        name_scope: &[Option<Arc<str>>],
    ) -> FunctionCall {
        let new_args = action
            .args
            .iter()
            .map(|arg| match arg {
                Expression::Capture(idx) => {
                    let val = scope.get(*idx).cloned().unwrap_or_default();
                    Expression::String(val)
                }
                Expression::CaptureName(name) => {
                    let val = name_scope
                        .iter()
                        .enumerate()
                        .find(|(_, n)| n.as_deref() == Some(name.as_str()))
                        .and_then(|(i, _)| scope.get(i))
                        .cloned()
                        .unwrap_or_default();
                    Expression::String(val)
                }
                Expression::Variable(v) => {
                    if let Some(inner) = self.resolve_variable_streaming(v) {
                        match &inner {
                            Expression::String(s) => Expression::String(s.clone()),
                            Expression::Number(n) => Expression::String(n.to_string()),
                            _ => arg.clone(),
                        }
                    } else {
                        arg.clone()
                    }
                }
                _ => arg.clone(),
            })
            .collect();
        FunctionCall {
            name: action.name.clone(),
            args: new_args,
        }
    }

    /// Execute runtime actions, recording streaming events and applying to the tree.
    fn execute_runtime_actions_streaming(
        &mut self,
        actions: &[FunctionCall],
        events: &mut Vec<StreamingEvent>,
    ) -> (i32, bool, Option<String>, usize) {
        let mut return_levels: i32 = 0;
        let mut did_next = false;
        let mut fail_error: Option<String> = None;
        let mut auto_leaves: usize = 0;

        for action in actions {
            let args: Vec<String> = action.args.iter().map(|a| self.expr_to_string_streaming(a)).collect();
            match &*action.name {
                "out.create" => {
                    if let Some(path) = args.first() {
                        let value = args.get(1).map(|s| s.as_str());
                        let ra = RuntimeAction::OutCreate { path, value };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                        self.fire_triggers_streaming("on_add", events);
                    }
                }
                "out.add" => {
                    if let Some(path) = args.first() {
                        let value = args.get(1).map(|s| s.as_str());
                        let ra = RuntimeAction::OutAdd { path, value };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                        self.fire_triggers_streaming("on_add", events);
                    }
                }
                "out.replace" => {
                    if let Some(path) = args.first() {
                        let value = args.get(1).map(|s| s.as_str());
                        let ra = RuntimeAction::OutReplace { path, value };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                    }
                }
                "out.add_attribute" => {
                    if args.len() >= 3 {
                        let ra = RuntimeAction::OutAddAttribute {
                            path: &args[0],
                            name: &args[1],
                            value: &args[2],
                        };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                    }
                }
                "out.set_root_name" | "out.name" => {
                    if let Some(name) = args.first() {
                        let ra = RuntimeAction::OutSetRootName { name };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                    }
                }
                "out.open" => {
                    if let Some(path) = args.first() {
                        let ra = RuntimeAction::OutOpen { path };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                        auto_leaves += 1;
                    }
                }
                "out.enter" => {
                    if let Some(path) = args.first() {
                        let ra = RuntimeAction::OutEnter { path };
                        events.push(StreamingEvent::from_action(&ra));
                        self.tree.exec(ra);
                        auto_leaves += 1;
                    }
                }
                "out.leave" => {
                    events.push(StreamingEvent::Leave);
                    self.tree.leave();
                }
                "do.return" => {
                    let levels = args.first().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                    return_levels = levels;
                }
                "do.next" => {
                    did_next = true;
                }
                "do.fail" => {
                    fail_error = Some(args.first().cloned().unwrap_or_else(|| "fail".to_string()));
                }
                "do.run" | "do.grammar" => {
                    if let Some(grammar_name) = args.first() {
                        // Inline grammar execution (non-streaming for sub-grammars).
                        let input = &self.buffer[self.pos..];
                        let mut sub_doc = self.doc.clone();
                        match super::execute(&mut sub_doc, grammar_name, input) {
                            Ok(sub_result) => {
                                // Merge sub-result events.
                                self.pos += sub_result.consumed;
                                self.consumed += sub_result.consumed;
                                events.push(StreamingEvent::MatchConsumed {
                                    pos: self.pos - sub_result.consumed,
                                    len: sub_result.consumed,
                                });
                            }
                            Err(e) => {
                                self.traces.push(format!("sub-grammar error: {}", e));
                            }
                        }
                    }
                }
                "enqueue_before"
                | "enqueue_after"
                | "enqueue_on_add"
                | "enqueue_on_leave"
                | "enqueue_before_persist"
                | "enqueue_after_persist"
                | "enqueue_on_add_persist"
                | "enqueue_on_leave_persist" => {
                    self.register_trigger_streaming(&action.name, &args);
                }
                _ => {
                    self.traces.push(format!("unknown action: {}", action.name));
                }
            }

            if return_levels > 0 {
                break;
            }
        }

        (return_levels, did_next, fail_error, auto_leaves)
    }

    fn expr_to_string_streaming(&self, expr: &Expression) -> String {
        match expr {
            Expression::String(s) => {
                let mut out = String::new();
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '$' {
                        let mut token = String::new();
                        while let Some(&p) = chars.peek() {
                            if p.is_ascii_alphanumeric() || p == '_' {
                                token.push(p);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if token.is_empty() {
                            out.push('$');
                        } else if token.chars().all(|d| d.is_ascii_digit()) {
                            if let Ok(idx) = token.parse::<usize>() {
                                if let Some(val) = self.last_captures.get(idx) {
                                    out.push_str(val);
                                }
                            }
                        } else if let Some((i, _)) = self
                            .last_capture_names
                            .iter()
                            .enumerate()
                            .find(|(_, n)| n.as_deref() == Some(token.as_str()))
                        {
                            if let Some(val) = self.last_captures.get(i) {
                                out.push_str(val);
                            }
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            }
            Expression::Number(n) => n.to_string(),
            Expression::Regex(r) => format!("/{}/", r),
            Expression::Variable(v) => {
                if let Some(inner) = self.resolve_variable_streaming(v) {
                    self.expr_to_string_streaming(&inner)
                } else {
                    String::new()
                }
            }
            Expression::Capture(_) | Expression::CaptureName(_) => String::new(),
        }
    }

    fn fire_triggers_streaming(&mut self, phase: &str, events: &mut Vec<StreamingEvent>) {
        let captures_refs: Vec<&str> = self.last_captures.iter().map(|c| c.as_str()).collect();
        let match_text = self.last_match_text.clone();

        // Collect matching single-shot triggers.
        let (single, persist) = match phase {
            "before" => (&mut self.trig_before, &self.trig_before_persist),
            "after" => (&mut self.trig_after, &self.trig_after_persist),
            "on_add" => (&mut self.trig_on_add, &self.trig_on_add_persist),
            "on_leave" => (&mut self.trig_on_leave, &self.trig_on_leave_persist),
            _ => return,
        };

        // Fire single-shot (drain matching).
        let mut remaining = Vec::new();
        for (rx, ta) in single.drain(..) {
            if rx.is_match(&match_text) {
                let value = ta
                    .value
                    .as_ref()
                    .map(|v| interpolate_local_streaming(v, &captures_refs));
                let path = interpolate_local_streaming(&ta.path, &captures_refs);
                let ra = RuntimeAction::OutCreate {
                    path: &path,
                    value: value.as_deref(),
                };
                events.push(StreamingEvent::from_action(&ra));
                self.tree.exec(RuntimeAction::OutCreate {
                    path: &path,
                    value: value.as_deref(),
                });
            } else {
                remaining.push((rx, ta));
            }
        }
        // Restore non-matching.
        match phase {
            "before" => self.trig_before = remaining,
            "after" => self.trig_after = remaining,
            "on_add" => self.trig_on_add = remaining,
            "on_leave" => self.trig_on_leave = remaining,
            _ => {}
        }

        // Fire persistent triggers.
        for (rx, ta) in persist {
            if rx.is_match(&match_text) {
                let value = ta
                    .value
                    .as_ref()
                    .map(|v| interpolate_local_streaming(v, &captures_refs));
                let path = interpolate_local_streaming(&ta.path, &captures_refs);
                let ra = RuntimeAction::OutCreate {
                    path: &path,
                    value: value.as_deref(),
                };
                events.push(StreamingEvent::from_action(&ra));
                self.tree.exec(RuntimeAction::OutCreate {
                    path: &path,
                    value: value.as_deref(),
                });
            }
        }
    }

    fn register_trigger_streaming(&mut self, name: &str, args: &[String]) {
        if args.len() < 2 {
            return;
        }
        let pattern_str = &args[0];
        let path = args[1].clone();
        let value = args.get(2).cloned();
        let rx = match Regex::new(pattern_str) {
            Ok(r) => r,
            Err(_) => return,
        };
        let ta = TriggerAction { path, value };
        match name {
            "enqueue_before" => self.trig_before.push((rx, ta)),
            "enqueue_after" => self.trig_after.push((rx, ta)),
            "enqueue_on_add" => self.trig_on_add.push((rx, ta)),
            "enqueue_on_leave" => self.trig_on_leave.push((rx, ta)),
            "enqueue_before_persist" => self.trig_before_persist.push((rx, ta)),
            "enqueue_after_persist" => self.trig_after_persist.push((rx, ta)),
            "enqueue_on_add_persist" => self.trig_on_add_persist.push((rx, ta)),
            "enqueue_on_leave_persist" => self.trig_on_leave_persist.push((rx, ta)),
            _ => {}
        }
    }
}

/// Interpolate `$N` references in a string using capture values.
fn interpolate_local_streaming(template: &str, captures: &[&str]) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'$') {
            // \$ → literal '$'
            out.push('$');
            chars.next();
        } else if c == '$' {
            let mut digits = String::new();
            while let Some(&p) = chars.peek() {
                if p.is_ascii_digit() {
                    digits.push(p);
                    chars.next();
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                out.push('$');
            } else if let Ok(idx) = digits.parse::<usize>() {
                if let Some(val) = captures.get(idx) {
                    out.push_str(val);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

//! Formulas a planner edits (decision 0077, workstream 7a of 0070): Starlark run on the reserves dataflow's units.
//!
//! A formula is a Starlark file defining `def formula(unit, p):`. It reads the unit's fields and labels
//! (`unit.ash_ad`, `unit.seam`), its working section and column added up (`unit.section.volume`), its block's
//! attributes (`unit.block.strip`), the unit above (`unit.above.area`) and the parameters (`p.rom_ash_cutoff`), and
//! sets fields the dataflow declares (`unit.rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)`). It never moves material
//! between units: that's a built-in step's work.
//!
//! The engine asks this crate to run a formula on units and doesn't know the language: a `Formula` is compiled once,
//! checked against the dataflow's names before it's saved, run over a column's units (columns in parallel), and traced
//! on one unit to show what it read, the values of each line as it ran, and what it set. Starlark has no files, network,
//! clock, randomness or imports, so the same formula on the same units always gives the same answer.

mod check;
mod handles;
pub mod library;

use std::fmt;
use std::sync::{Arc, Mutex};

use starlark::codemap::FileSpanRef;
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module};
use starlark::eval::{BeforeStmtFunc, BeforeStmtFuncDyn, Evaluator};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::{OwnedFrozenValue, Value, ValueLike};
use starlark::PrintHandler;

pub use handles::Event;
use handles::{Params, Parts, Unit};

/// A value a formula reads or sets: a number, words, or nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptValue {
    None,
    Number(f64),
    Text(String),
}

impl fmt::Display for ScriptValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptValue::None => write!(f, "None"),
            ScriptValue::Number(x) => write!(f, "{x}"),
            ScriptValue::Text(s) => write!(f, "{s:?}"),
        }
    }
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Text,
}

/// A field or parameter column a formula may use, with its unit (`m3`, `%`, `t/m3`).
#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub kind: Kind,
    pub unit: String,
}

impl FieldDecl {
    pub fn number(name: &str, unit: &str) -> Self {
        Self { name: name.into(), kind: Kind::Number, unit: unit.into() }
    }
    pub fn text(name: &str) -> Self {
        Self { name: name.into(), kind: Kind::Text, unit: String::new() }
    }
}

/// The names a dataflow gives its formulas: the fields a unit carries (which a formula may set), the labels it reads
/// but never sets (seam, material, bench), and the columns of its parameter tables.
#[derive(Debug, Clone, Default)]
pub struct Declared {
    pub fields: Vec<FieldDecl>,
    pub labels: Vec<String>,
    pub parameters: Vec<FieldDecl>,
}

impl Declared {
    pub fn is_field(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name)
    }
    pub fn is_label(&self, name: &str) -> bool {
        self.labels.iter().any(|l| l == name)
    }
}

/// Named values, shared between the units that have them (a section's add-ups, a parameter row's values).
pub type Values = Arc<Vec<(String, ScriptValue)>>;

/// Builds `Values` from pairs.
pub fn values<'a>(pairs: impl IntoIterator<Item = (&'a str, ScriptValue)>) -> Values {
    Arc::new(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// A unit as the engine hands it over: its fields and labels, its block's attributes, its parameters, and which of the
/// column's working sections it's in.
#[derive(Debug, Clone)]
pub struct UnitInput {
    pub fields: Values,
    pub block: Values,
    pub parameters: Values,
    pub section: Option<usize>,
}

/// A block column, its units top down, each working section's fields added up by their rules, and the column's.
#[derive(Debug, Clone, Default)]
pub struct ColumnInput {
    pub units: Vec<UnitInput>,
    pub sections: Vec<Values>,
    pub column: Values,
}

/// What a formula set on a unit, in the order it set them (the last of a field's is its value), and what it printed.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSets {
    pub unit: usize,
    pub sets: Vec<(String, ScriptValue)>,
    pub said: Vec<String>,
}

impl UnitSets {
    /// A field's value as the formula left it.
    pub fn get(&self, field: &str) -> Option<&ScriptValue> {
        self.sets.iter().rev().find(|(k, _)| k == field).map(|(_, v)| v)
    }
}

/// Why a formula can't be saved or stopped on a unit: what's wrong, its line (from 1), and the unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub message: String,
    pub line: Option<usize>,
    pub unit: Option<usize>,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "line {line}: ")?;
        }
        write!(f, "{}", self.message)?;
        if let Some(unit) = self.unit {
            write!(f, " (unit {})", unit + 1)?;
        }
        Ok(())
    }
}

/// A line of a formula as it ran on one unit: its text, and the names it gave a value with their values.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceLine {
    pub line: usize,
    pub text: String,
    pub values: Vec<(String, String)>,
}

/// A formula run on one unit, shown: the lines that ran in order (a line not reached isn't there), what it read and set,
/// what it printed, and where it stopped if it did.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub lines: Vec<TraceLine>,
    pub events: Vec<Event>,
    pub said: Vec<String>,
    pub sets: Vec<(String, ScriptValue)>,
    pub stopped: Option<Problem>,
}

const FILE: &str = "formula.star";

fn globals() -> Globals {
    GlobalsBuilder::extended_by(&[LibraryExtension::StructType, LibraryExtension::Map, LibraryExtension::Filter, LibraryExtension::Print, LibraryExtension::Json])
        .with(library::library)
        .build()
}

/// A Starlark error as a planner reads it: its message without the "error:" prefix, our words for a missing parameter
/// or field, and its line in the formula.
fn problem(e: &starlark::Error, unit: Option<usize>) -> Problem {
    let full = format!("{e}");
    // A runtime error comes after its traceback, on the line that starts "error:".
    let headline = full.lines().find(|l| l.starts_with("error:")).or_else(|| full.lines().find(|l| !l.trim().is_empty())).unwrap_or("");
    // `fail("…")` is the formula saying why it stopped: its words alone.
    let mut message = headline.trim_start_matches("error:").trim().trim_start_matches("fail:").trim().to_string();
    let line = full.lines().filter_map(|l| l.split_once(&format!("{FILE}:"))).filter_map(|(_, rest)| rest.split(':').next()?.trim().parse::<usize>().ok()).next_back();
    if let Some(name) = message.strip_prefix("Object of type `parameters` has no attribute `").and_then(|r| r.split('`').next()) {
        message = format!("no parameter row gives `{name}` for this unit; read it with getattr(p, \"{name}\", default) where it may be missing");
    } else if let Some(name) = message.strip_prefix("Object of type `unit` has no attribute `").and_then(|r| r.split('`').next()) {
        message = format!("`{name}` isn't a field of this dataflow");
    }
    Problem { message, line, unit }
}

/// Collects what a formula prints.
#[derive(Default)]
struct Said(Mutex<Vec<String>>);

impl PrintHandler for Said {
    fn println(&self, text: &str) -> starlark::Result<()> {
        self.0.lock().expect("said").push(text.to_string());
        Ok(())
    }
}

impl Said {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().expect("said"))
    }
}

/// A formula compiled and checked against a dataflow's names.
pub struct Formula {
    source: String,
    declared: Arc<Declared>,
    takes_p: bool,
    globals: Globals,
    _module: FrozenModule,
    function: OwnedFrozenValue,
}

impl fmt::Debug for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Formula").field("lines", &self.source.lines().count()).field("takes_p", &self.takes_p).finish()
    }
}

impl Formula {
    /// Parses, loads and checks a formula. Every problem is said with its line; any one refuses it.
    pub fn compile(source: &str, declared: Declared) -> Result<Formula, Vec<Problem>> {
        let declared = Arc::new(declared);
        let globals = globals();
        let ast = AstModule::parse(FILE, source.to_owned(), &Dialect::Standard).map_err(|e| vec![problem(&e, None)])?;
        let mut problems = Vec::new();
        let sig = check::signature(source);
        let takes_p = match &sig {
            None => {
                problems.push(Problem { message: "a formula defines its function as `def formula(unit, p):` (p only where it reads parameters)".into(), line: None, unit: None });
                false
            }
            Some((line, unit, p)) if unit.is_empty() => {
                problems.push(Problem { message: "`formula` takes the unit, and the parameters where it reads them: `def formula(unit, p):`".into(), line: Some(*line), unit: None });
                p.is_some()
            }
            Some((_, unit, p)) => {
                problems.extend(check::names(source, &declared, unit, p.as_deref()));
                p.is_some()
            }
        };
        let loaded = Module::with_temp_heap(|module| -> Result<FrozenModule, Problem> {
            {
                let said = Said::default();
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&said);
                eval.eval_module(ast, &globals).map_err(|e| problem(&e, None))?;
            }
            module.freeze().map_err(|e| Problem { message: format!("{e:?}"), line: None, unit: None })
        });
        let module = match loaded {
            Ok(m) => m,
            Err(p) => {
                problems.insert(0, p);
                problems.sort_by_key(|p| p.line.unwrap_or(0));
                return Err(problems);
            }
        };
        let function = module.get("formula").ok().filter(|f| f.value().get_type() == "function");
        if function.is_none() && sig.is_some() {
            problems.push(Problem { message: "`formula` isn't a function".into(), line: sig.as_ref().map(|s| s.0), unit: None });
        }
        if !problems.is_empty() {
            problems.sort_by_key(|p| p.line.unwrap_or(0));
            return Err(problems);
        }
        let Some(function) = function else {
            return Err(vec![Problem { message: "a formula defines `def formula(unit, p):`".into(), line: None, unit: None }]);
        };
        Ok(Formula { source: source.to_string(), declared, takes_p, globals, _module: module, function })
    }

    /// The formula as written.
    pub fn source(&self) -> &str {
        &self.source
    }

    fn unit_handle(&self, column: &ColumnInput, u: usize, log: Option<handles::Log>) -> Unit {
        let unit = &column.units[u];
        Unit {
            fields: unit.fields.clone(),
            section: unit.section.and_then(|s| column.sections.get(s).cloned()),
            column: column.column.clone(),
            block: unit.block.clone(),
            above: u.checked_sub(1).map(|a| column.units[a].fields.clone()),
            declared: self.declared.clone(),
            sets: Mutex::new(Vec::new()),
            log,
        }
    }

    fn call<'v>(&self, eval: &mut Evaluator<'v, '_, '_>, f: Value<'v>, column: &ColumnInput, u: usize, log: Option<handles::Log>) -> Result<Vec<(String, ScriptValue)>, Problem> {
        let heap = eval.heap();
        let unit = heap.alloc(self.unit_handle(column, u, log.clone()));
        let result = if self.takes_p {
            let p = heap.alloc(Params { values: column.units[u].parameters.clone(), log });
            eval.eval_function(f, &[unit, p], &[])
        } else {
            eval.eval_function(f, &[unit], &[])
        };
        result.map_err(|e| problem(&e, Some(u)))?;
        let handle = unit.downcast_ref::<Unit>().expect("a unit");
        Ok(std::mem::take(&mut *handle.sets.lock().expect("sets")))
    }

    /// Runs the formula on some of a column's units, top down; the first problem stops the column.
    pub fn run(&self, column: &ColumnInput, units: &[usize]) -> Result<Vec<UnitSets>, Problem> {
        let said = Said::default();
        Module::with_temp_heap(|module| {
            // SAFETY: the formula's function was frozen when it compiled; handing this module's frozen heap a reference
            // to the heap it lives on keeps it alive for as long as the module, which outlives every call made here.
            let f = unsafe { self.function.owned_frozen_value(module.frozen_heap()) }.to_value();
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            let mut out = Vec::with_capacity(units.len());
            for &u in units {
                let sets = self.call(&mut eval, f, column, u, None)?;
                out.push(UnitSets { unit: u, sets, said: said.take() });
            }
            Ok(out)
        })
    }

    /// Runs the formula on many columns, in parallel, each as `run` does; the results in the order given.
    pub fn run_many(&self, jobs: &[(&ColumnInput, &[usize])]) -> Vec<Result<Vec<UnitSets>, Problem>> {
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len()).max(1);
        let mut results: Vec<Option<Result<Vec<UnitSets>, Problem>>> = (0..jobs.len()).map(|_| None).collect();
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|w| scope.spawn(move || (w..jobs.len()).step_by(workers).map(|i| (i, self.run(jobs[i].0, jobs[i].1))).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                for (i, r) in h.join().expect("a formula worker") {
                    results[i] = Some(r);
                }
            }
        });
        results.into_iter().map(|r| r.expect("every job run")).collect()
    }

    /// Runs the formula on one unit and shows it: the lines that ran with the values each gave, what it read and set,
    /// what it printed, and the problem that stopped it if one did. Nothing is kept.
    pub fn trace(&self, column: &ColumnInput, u: usize) -> Trace {
        let snapshots: Arc<Mutex<Vec<Snapshot>>> = Arc::default();
        let log: handles::Log = Arc::default();
        let said = Said::default();
        let mut sets = Vec::new();
        let stopped = Module::with_temp_heap(|module| -> Option<Problem> {
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            // The hook is set before the formula is loaded, so its statements are compiled to call it.
            eval.before_stmt_for_dap(BeforeStmtFunc::from_dyn(Box::new(Watch(snapshots.clone()))));
            let ast = AstModule::parse(FILE, self.source.clone(), &Dialect::Standard).expect("it compiled before");
            if let Err(e) = eval.eval_module(ast, &self.globals) {
                return Some(problem(&e, Some(u)));
            }
            snapshots.lock().expect("snapshots").clear();
            let f = module.get("formula").expect("it compiled before");
            match self.call(&mut eval, f, column, u, Some(log.clone())) {
                Ok(s) => {
                    sets = s;
                    None
                }
                Err(p) => Some(p),
            }
        });
        let snapshots = std::mem::take(&mut *snapshots.lock().expect("snapshots"));
        let source: Vec<&str> = self.source.lines().collect();
        let lines = snapshots
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // What this line gave: the names that changed by the next statement in the same function.
                let next = snapshots[i + 1..].iter().take_while(|n| n.depth >= s.depth).find(|n| n.depth == s.depth);
                let values = next
                    .map(|n| n.locals.iter().filter(|(k, v)| s.locals.iter().find(|(k2, _)| k2 == k).is_none_or(|(_, v2)| v2 != v)).cloned().collect())
                    .unwrap_or_default();
                TraceLine { line: s.line, text: source.get(s.line - 1).map(|t| t.trim().to_string()).unwrap_or_default(), values }
            })
            .collect();
        let events = std::mem::take(&mut *log.lock().expect("log"));
        Trace { lines, events, said: said.take(), sets, stopped }
    }
}

/// The locals as a statement was about to run.
struct Snapshot {
    line: usize,
    depth: usize,
    locals: Vec<(String, String)>,
}

struct Watch(Arc<Mutex<Vec<Snapshot>>>);

impl<'e> BeforeStmtFuncDyn<'e> for Watch {
    fn call<'v>(&mut self, span: FileSpanRef, _continued: bool, eval: &mut Evaluator<'v, '_, 'e>) -> starlark::Result<()> {
        let line = span.resolve_span().begin.line + 1;
        let locals = eval.local_variables().into_iter().map(|(k, v)| (k, show(v))).collect();
        self.0.lock().expect("snapshots").push(Snapshot { line, depth: eval.call_stack_count(), locals });
        Ok(())
    }
}

/// A value as the trace shows it.
fn show(v: Value<'_>) -> String {
    if let Some(u) = v.downcast_ref::<Unit>() {
        return u.to_string();
    }
    if v.downcast_ref::<Parts>().is_some() || v.downcast_ref::<Params>().is_some() {
        return v.to_string();
    }
    v.to_repr()
}

#[cfg(test)]
mod tests;

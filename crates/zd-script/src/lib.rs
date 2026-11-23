//! Formulas a planner edits (decision 0077, workstream 7a of 0070): Starlark run on the reserves dataflow's units.
//!
//! A formula is a Starlark file defining `def formula(unit, p):`. It reads the unit's fields and labels
//! (`unit.ash_ad`, `unit.seam`), its working section, its ROM section's coal and its column added up
//! (`unit.section.volume`, `unit.rom_coal.mad`), its block's
//! attributes (`unit.block.strip`), the unit above (`unit.above.area`) and the parameters (`p.rom_ash_cutoff`), and
//! sets fields the dataflow declares (`unit.rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)`). It never moves material
//! between units: that's a built-in step's work.
//!
//! The engine asks this crate to run a formula on units and doesn't know the language: a `Formula` is compiled once,
//! checked against the dataflow's names before it's saved, run over a column's units (columns in parallel), and traced
//! on one unit to show what it read, the values of each line as it ran, and what it set. Starlark has no files, network,
//! clock, randomness or imports, so the same formula on the same units always gives the same answer.

mod alloc;
mod check;
mod handles;
pub mod library;
pub mod help;
mod checks;
mod lookups;
mod tables;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use starlark::codemap::FileSpanRef;
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module};
use starlark::eval::{BeforeStmtFunc, BeforeStmtFuncDyn, Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::{OwnedFrozenValue, Value, ValueLike};
use starlark::PrintHandler;

pub use alloc::{Allocated, AllocationInput, Ends, SplitMade};
pub use handles::{Event, Placed};
pub use lookups::Lookups;
pub use tables::{TableWork, add_up_from_words};

/// How to write a formula, for the formula panel and for an AI asked to write one (decision 0077, 7d and 7e).
pub const REFERENCE: &str = include_str!("reference.md");
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

/// The project's scripts a formula may load (decision 0099, workstream 6): each its name and its Starlark.
pub type Libraries = Arc<Vec<(String, String)>>;

/// The names a dataflow gives its formulas: the fields a unit carries (which a formula may set), the labels it reads
/// but never sets (seam, material, bench), the columns of its parameter tables, and the scripts it may load by name.
#[derive(Debug, Clone, Default)]
pub struct Declared {
    pub fields: Vec<FieldDecl>,
    pub labels: Vec<String>,
    pub parameters: Vec<FieldDecl>,
    pub libraries: Libraries,
    /// The parameter tables it may look up (Schedule quantities plan, decision 1): its own for `p.at`, and those named
    /// in `lookup("…")`, found by name.
    pub lookups: Arc<Lookups>,
}

/// What a script's run carries for the functions that need it: the tables it looks up, the trace's log, and a table
/// script's tables.
#[derive(starlark::any::ProvidesStaticType)]
pub(crate) struct ScriptRun {
    pub lookups: Arc<Lookups>,
    pub log: Option<handles::Log>,
    pub tables: Option<tables::TableRun>,
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
    /// The ROM coal section it's in (coal, partings, dilution or coal from a neighbour), an index into `rom_coals`.
    pub rom_coal: Option<usize>,
}

/// A block column, its units top down, each working section's fields added up by their rules, each ROM coal section's
/// coal (its plies, nothing done to them) added up, and the column's.
#[derive(Debug, Clone, Default)]
pub struct ColumnInput {
    pub units: Vec<UnitInput>,
    pub sections: Vec<Values>,
    pub rom_coals: Vec<Values>,
    pub column: Values,
}

/// What a formula set on a unit, in the order it set them (the last of a field's is its value), and what it printed.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSets {
    pub unit: usize,
    pub sets: Vec<(String, ScriptValue)>,
    /// What it placed onto other leaves of the block, in order (Waste allocation plan, decision 5).
    pub placed: Vec<Placed>,
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
        .with(tables::table_globals)
        .with(checks::check_globals)
        .with(lookups::lookup_globals)
        .with(lookups::curve_globals)
        .build()
}

/// `load("name", "f")` among the project's scripts (decision 0099, workstream 6): each script loaded once a run, with the
/// same library as a formula, and able to load others. A name the project hasn't, or a script that loads itself (on
/// its own or through others), is refused. Never a file, the network or an import.
struct ProjectLoader<'a> {
    libraries: &'a Libraries,
    globals: &'a Globals,
    loading: RefCell<Vec<String>>,
    loaded: RefCell<HashMap<String, FrozenModule>>,
}

impl<'a> ProjectLoader<'a> {
    fn new(libraries: &'a Libraries, globals: &'a Globals) -> Self {
        ProjectLoader { libraries, globals, loading: RefCell::default(), loaded: RefCell::default() }
    }
}

impl FileLoader for ProjectLoader<'_> {
    fn load(&self, path: &str) -> starlark::Result<FrozenModule> {
        if let Some(m) = self.loaded.borrow().get(path) {
            return Ok(m.clone());
        }
        let fail = |why: String| starlark::Error::new_other(anyhow::anyhow!(why));
        let Some((_, source)) = self.libraries.iter().find(|(name, _)| name == path) else {
            return Err(fail(format!("can't load {path}: the project has no script of that name")));
        };
        if self.loading.borrow().iter().any(|n| n == path) {
            let chain = self.loading.borrow().join(" → ");
            return Err(fail(format!("{path} loads itself, through {chain} → {path}")));
        }
        self.loading.borrow_mut().push(path.to_string());
        let source = source.replace("\r\n", "\n");
        let frozen = AstModule::parse(&format!("{path}.star"), source, &Dialect::Standard)
            .map_err(|e| fail(format!("the script {path}: {}", first_line(&e))))
            .and_then(|ast| {
                Module::with_temp_heap(|module| {
                    {
                        let said = Said::default();
                        let mut eval = Evaluator::new(&module);
                        eval.set_print_handler(&said);
                        eval.set_loader(self);
                        eval.eval_module(ast, self.globals).map_err(|e| fail(format!("the script {path}: {}", first_line(&e))))?;
                    }
                    module.freeze().map_err(|e| fail(format!("the script {path}: {e:?}")))
                })
            });
        self.loading.borrow_mut().pop();
        let frozen = frozen?;
        self.loaded.borrow_mut().insert(path.to_string(), frozen.clone());
        Ok(frozen)
    }
}

/// A Starlark error's own line: its message, and where in its script.
fn first_line(e: &starlark::Error) -> String {
    let full = format!("{e}");
    let headline = full.lines().find(|l| l.starts_with("error:")).or_else(|| full.lines().find(|l| !l.trim().is_empty())).unwrap_or("").trim_start_matches("error:").trim().to_string();
    let at = full.lines().filter_map(|l| l.split_once(".star:")).filter_map(|(_, rest)| rest.split(':').next()?.trim().parse::<usize>().ok()).next_back();
    match at {
        Some(line) => format!("line {line}: {headline}"),
        None => headline,
    }
}

/// A script of the project checked as a library (decision 0099, workstream 6): it parses, and everything it loads is
/// there, loads cleanly and doesn't load it back. Every problem is said with its line.
pub fn check_library(name: &str, source: &str, libraries: &Libraries) -> Result<(), Vec<Problem>> {
    let source = source.replace("\r\n", "\n");
    let mut others: Vec<(String, String)> = libraries.iter().filter(|(n, _)| n != name).cloned().collect();
    others.push((name.to_string(), source.clone()));
    let libraries: Libraries = Arc::new(others);
    let globals = globals();
    let loader = ProjectLoader::new(&libraries, &globals);
    loader.load(name).map(|_| ()).map_err(|e| {
        let message = format!("{e}");
        let message = message.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim_start_matches("error:").trim().to_string();
        let line = message.strip_prefix(&format!("the script {name}: line ")).and_then(|r| r.split(':').next()?.parse().ok());
        vec![Problem { message, line, unit: None }]
    })
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
    } else if let Some(name) = message.strip_prefix("Object of type `table` has no attribute `").and_then(|r| r.split('`').next()) {
        message = format!("a table has no `{name}`: it has rows, columns, name, t[\"column\"], where, group, total, join, select, drop, rename, with_column, sort, row and unit");
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

/// What a formula set on a unit, in order.
type Sets = Vec<(String, ScriptValue)>;

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
        // A script pasted or checked out on Windows ends its lines with \r\n: read as plain new lines.
        let source = &source.replace("\r\n", "\n");
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
        let loader = ProjectLoader::new(&declared.libraries, &globals);
        let loaded = Module::with_temp_heap(|module| -> Result<FrozenModule, Problem> {
            {
                let said = Said::default();
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&said);
                eval.set_loader(&loader);
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

    /// The dataflow's fields it reads, read off its source, each once in the order first read (decision 0084): what the
    /// Dataflow view lights in Before and After.
    pub fn fields_read(&self) -> Vec<String> {
        check::signature(&self.source).map_or_else(Vec::new, |(_, unit, _)| check::fields_read(&self.source, &self.declared, &unit))
    }

    fn unit_handle(&self, column: &ColumnInput, u: usize, log: Option<handles::Log>) -> Unit {
        let unit = &column.units[u];
        Unit {
            fields: unit.fields.clone(),
            section: unit.section.and_then(|s| column.sections.get(s).cloned()),
            column: column.column.clone(),
            rom_coal: unit.rom_coal.and_then(|r| column.rom_coals.get(r).cloned()),
            block: unit.block.clone(),
            above: u.checked_sub(1).map(|a| column.units[a].fields.clone()),
            declared: self.declared.clone(),
            sets: Mutex::new(Vec::new()),
            placed: Mutex::new(Vec::new()),
            log,
        }
    }

    fn call<'v>(&self, eval: &mut Evaluator<'v, '_, '_>, f: Value<'v>, column: &ColumnInput, u: usize, log: Option<handles::Log>) -> Result<(Sets, Vec<Placed>), Problem> {
        let heap = eval.heap();
        let unit = heap.alloc(self.unit_handle(column, u, log.clone()));
        let result = if self.takes_p {
            let p = heap.alloc(Params { values: column.units[u].parameters.clone(), log, lookups: Some(self.declared.lookups.clone()), reads: None });
            eval.eval_function(f, &[unit, p], &[])
        } else {
            eval.eval_function(f, &[unit], &[])
        };
        result.map_err(|e| problem(&e, Some(u)))?;
        let handle = unit.downcast_ref::<Unit>().expect("a unit");
        Ok((std::mem::take(&mut *handle.sets.lock().expect("sets")), std::mem::take(&mut *handle.placed.lock().expect("placed"))))
    }

    /// Runs the formula on some of a column's units, top down; the first problem stops the column.
    pub fn run(&self, column: &ColumnInput, units: &[usize]) -> Result<Vec<UnitSets>, Problem> {
        let said = Said::default();
        Module::with_temp_heap(|module| {
            // SAFETY: the formula's function was frozen when it compiled; handing this module's frozen heap a reference
            // to the heap it lives on keeps it alive for as long as the module, which outlives every call made here.
            let f = unsafe { self.function.owned_frozen_value(module.frozen_heap()) }.to_value();
            let context = ScriptRun { lookups: self.declared.lookups.clone(), log: None, tables: None };
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            eval.extra = Some(&context);
            let mut out = Vec::with_capacity(units.len());
            for &u in units {
                let (sets, placed) = self.call(&mut eval, f, column, u, None)?;
                out.push(UnitSets { unit: u, sets, placed, said: said.take() });
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
        let loader = ProjectLoader::new(&self.declared.libraries, &self.globals);
        let context = ScriptRun { lookups: self.declared.lookups.clone(), log: Some(log.clone()), tables: None };
        let stopped = Module::with_temp_heap(|module| -> Option<Problem> {
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            eval.set_loader(&loader);
            eval.extra = Some(&context);
            // The hook is set before the formula is loaded, so its statements are compiled to call it.
            eval.before_stmt_for_dap(BeforeStmtFunc::from_dyn(Box::new(Watch(snapshots.clone()))));
            let ast = AstModule::parse(FILE, self.source.clone(), &Dialect::Standard).expect("it compiled before");
            if let Err(e) = eval.eval_module(ast, &self.globals) {
                return Some(problem(&e, Some(u)));
            }
            snapshots.lock().expect("snapshots").clear();
            let f = module.get("formula").expect("it compiled before");
            match self.call(&mut eval, f, column, u, Some(log.clone())) {
                Ok((s, _)) => {
                    sets = s;
                    None
                }
                Err(p) => Some(p),
            }
        });
        let lines = traced_lines(&self.source, std::mem::take(&mut *snapshots.lock().expect("snapshots")));
        let events = std::mem::take(&mut *log.lock().expect("log"));
        Trace { lines, events, said: said.take(), sets, stopped }
    }
}

/// The lines that ran, from the snapshots taken before each statement: each with the names it gave a value.
fn traced_lines(source: &str, snapshots: Vec<Snapshot>) -> Vec<TraceLine> {
    let source: Vec<&str> = source.lines().collect();
    snapshots
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
        .collect()
}

/// An allocation step compiled (the Waste allocation plan, decisions 3 and 4): Starlark defining
/// `def allocate(column, p):`, run once on a block column's units after `rom`. It reads and sets the units' fields and
/// words, splits units by thickness from the top or by volume shares (in Rust, the pieces adding back exactly), and
/// places schedule fields onto other leaves; `p` is the column's parameters, `unit.p` a unit's.
pub struct Allocation {
    source: String,
    declared: Arc<Declared>,
    takes_p: bool,
    globals: Globals,
    _module: FrozenModule,
    function: OwnedFrozenValue,
}

impl fmt::Debug for Allocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Allocation").field("lines", &self.source.lines().count()).field("takes_p", &self.takes_p).finish()
    }
}

/// Whether a script is an allocation script: it defines `allocate`.
pub fn is_allocation_script(source: &str) -> bool {
    check::signature_of(source, "allocate").is_some()
}

impl Allocation {
    /// Parses, loads and checks an allocation script. Every problem is said with its line; any one refuses it.
    pub fn compile(source: &str, declared: Declared) -> Result<Allocation, Vec<Problem>> {
        let source = source.replace("\r\n", "\n");
        let declared = Arc::new(declared);
        let globals = globals();
        let ast = AstModule::parse(FILE, source.clone(), &Dialect::Standard).map_err(|e| vec![problem(&e, None)])?;
        let takes_p = match check::signature_of(&source, "allocate") {
            None => return Err(vec![Problem { message: "an allocation step defines its function as `def allocate(column, p):` (p only where it reads the column's parameters)".into(), line: None, unit: None }]),
            Some((line, c, _)) if c.is_empty() => return Err(vec![Problem { message: "`allocate` takes the column, and the parameters where it reads them: `def allocate(column, p):`".into(), line: Some(line), unit: None }]),
            Some((_, _, p)) => p.is_some(),
        };
        let loader = ProjectLoader::new(&declared.libraries, &globals);
        let module = Module::with_temp_heap(|module| -> Result<FrozenModule, Problem> {
            {
                let said = Said::default();
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&said);
                eval.set_loader(&loader);
                eval.eval_module(ast, &globals).map_err(|e| problem(&e, None))?;
            }
            module.freeze().map_err(|e| Problem { message: format!("{e:?}"), line: None, unit: None })
        })
        .map_err(|p| vec![p])?;
        let Some(function) = module.get("allocate").ok().filter(|f| f.value().get_type() == "function") else {
            return Err(vec![Problem { message: "`allocate` isn't a function: an allocation step defines `def allocate(column, p):`".into(), line: None, unit: None }]);
        };
        Ok(Allocation { source, declared, takes_p, globals, _module: module, function })
    }

    /// The script as written.
    pub fn source(&self) -> &str {
        &self.source
    }

    fn begin(&self, input: AllocationInput, log: Option<handles::Log>) -> alloc::Run {
        let state = alloc::State { column: input.column, ends: input.ends, blocks: input.blocks, parameters: input.parameters, splits: Vec::new(), placed: Vec::new() };
        alloc::Run {
            state: Arc::new(Mutex::new(state)),
            declared: self.declared.clone(),
            log,
            reads: Arc::default(),
            name: input.name,
            column_block: input.column_block,
            column_parameters: input.column_parameters,
        }
    }

    fn call<'v>(&self, eval: &mut Evaluator<'v, '_, '_>, f: Value<'v>, run: &alloc::Run) -> Result<(), Problem> {
        let heap = eval.heap();
        let column = heap.alloc(alloc::ColumnHandle { run: run.clone() });
        let result = if self.takes_p {
            let p = heap.alloc(Params { values: run.column_parameters.clone(), log: run.log.clone(), lookups: Some(self.declared.lookups.clone()), reads: Some((run.reads.clone(), None)) });
            eval.eval_function(f, &[column, p], &[])
        } else {
            eval.eval_function(f, &[column], &[])
        };
        result.map(|_| ()).map_err(|e| problem(&e, None))
    }

    fn done(run: alloc::Run, said: Vec<String>) -> Allocated {
        let mut s = run.state.lock().expect("allocation column");
        let read = std::mem::take(&mut *run.reads.lock().expect("reads"));
        Allocated { column: s.column.clone(), splits: std::mem::take(&mut s.splits), placed: std::mem::take(&mut s.placed), said, read }
    }

    /// Runs the script on a column. The first problem stops it, said with its line.
    pub fn run(&self, input: AllocationInput) -> Result<Allocated, Problem> {
        let said = Said::default();
        let run = self.begin(input, None);
        Module::with_temp_heap(|module| {
            // SAFETY: as `Formula::run`: the function was frozen when it compiled, and this module's frozen heap keeps a
            // reference to the heap it lives on for as long as the module, which outlives the call made here.
            let f = unsafe { self.function.owned_frozen_value(module.frozen_heap()) }.to_value();
            let context = ScriptRun { lookups: self.declared.lookups.clone(), log: None, tables: None };
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            eval.extra = Some(&context);
            self.call(&mut eval, f, &run)
        })?;
        Ok(Self::done(run, said.take()))
    }

    /// Runs the script on a column and shows it: every line that ran in order with the values it gave, what it read and
    /// set, what it printed and where it stopped; with what it left the column as, where it ran through.
    pub fn trace(&self, input: AllocationInput) -> (Trace, Option<Allocated>) {
        let snapshots: Arc<Mutex<Vec<Snapshot>>> = Arc::default();
        let log: handles::Log = Arc::default();
        let said = Said::default();
        let run = self.begin(input, Some(log.clone()));
        let loader = ProjectLoader::new(&self.declared.libraries, &self.globals);
        let context = ScriptRun { lookups: self.declared.lookups.clone(), log: Some(log.clone()), tables: None };
        let stopped = Module::with_temp_heap(|module| -> Option<Problem> {
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            eval.set_loader(&loader);
            eval.extra = Some(&context);
            eval.before_stmt_for_dap(BeforeStmtFunc::from_dyn(Box::new(Watch(snapshots.clone()))));
            let ast = AstModule::parse(FILE, self.source.clone(), &Dialect::Standard).expect("it compiled before");
            if let Err(e) = eval.eval_module(ast, &self.globals) {
                return Some(problem(&e, None));
            }
            snapshots.lock().expect("snapshots").clear();
            let f = module.get("allocate").expect("it compiled before");
            self.call(&mut eval, f, &run).err()
        });
        let lines = traced_lines(&self.source, std::mem::take(&mut *snapshots.lock().expect("snapshots")));
        let events = std::mem::take(&mut *log.lock().expect("log"));
        let sets = events.iter().filter_map(|e| match e { Event::Set { field, value } => Some((field.clone(), value.clone())), _ => None }).collect();
        let said = said.take();
        let allocated = stopped.is_none().then(|| Self::done(run, said.clone()));
        (Trace { lines, events, said, sets, stopped }, allocated)
    }
}

/// A table script compiled (the Build Plan's "Tables and scripts plan", decisions 7 and 11): Starlark defining
/// `def run(t, p):`, given a table and giving one back. It runs as a task (`table.script`), as a dataflow step after the
/// final writes, or as a workflow step, the same script each way.
pub struct TableScript {
    source: String,
    takes_p: bool,
    _module: FrozenModule,
    function: OwnedFrozenValue,
}

impl fmt::Debug for TableScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableScript").field("lines", &self.source.lines().count()).field("takes_p", &self.takes_p).finish()
    }
}

/// What a table script is run on: its table and name, the project's tables `table("name")` may read, the parameters `p`
/// gives, and the engine's work.
pub struct TableInput {
    pub name: String,
    pub table: Arc<zd_core::Table>,
    pub tables: Vec<(String, Arc<zd_core::Table>)>,
    pub parameters: Values,
    pub work: Arc<dyn TableWork>,
    /// The parameter tables it may look up: those `p` reads, and those named in `lookup("…")`.
    pub lookups: Arc<Lookups>,
}

/// What a table script gave back: its table, what it printed and what the engine said of it (a column with no rule, a
/// join's unmatched rows), and the project's tables it read by name.
#[derive(Debug, Clone)]
pub struct TableRan {
    pub table: zd_core::Table,
    pub said: Vec<String>,
    pub notes: Vec<String>,
    pub read: Vec<String>,
    /// What its checks found out of line (`check()`, `required()`), a line a check, each leaf out named.
    pub checks: Vec<String>,
}

/// Whether a script is a table script: it defines `run`.
pub fn is_table_script(source: &str) -> bool {
    check::signature_of(source, "run").is_some()
}

impl TableScript {
    /// Parses, loads and checks a table script. Every problem is said with its line; any one refuses it.
    pub fn compile(source: &str, libraries: Libraries) -> Result<TableScript, Vec<Problem>> {
        let source = source.replace("\r\n", "\n");
        let globals = globals();
        let ast = AstModule::parse(FILE, source.clone(), &Dialect::Standard).map_err(|e| vec![problem(&e, None)])?;
        let takes_p = match check::signature_of(&source, "run") {
            None => return Err(vec![Problem { message: "a table script defines its function as `def run(t, p):` (p only where it reads parameters), and gives back a table".into(), line: None, unit: None }]),
            Some((line, t, _)) if t.is_empty() => return Err(vec![Problem { message: "`run` takes the table, and the parameters where it reads them: `def run(t, p):`".into(), line: Some(line), unit: None }]),
            Some((_, _, p)) => p.is_some(),
        };
        let loader = ProjectLoader::new(&libraries, &globals);
        let module = Module::with_temp_heap(|module| -> Result<FrozenModule, Problem> {
            {
                let said = Said::default();
                let mut eval = Evaluator::new(&module);
                eval.set_print_handler(&said);
                eval.set_loader(&loader);
                eval.eval_module(ast, &globals).map_err(|e| problem(&e, None))?;
            }
            module.freeze().map_err(|e| Problem { message: format!("{e:?}"), line: None, unit: None })
        })
        .map_err(|p| vec![p])?;
        let Some(function) = module.get("run").ok().filter(|f| f.value().get_type() == "function") else {
            return Err(vec![Problem { message: "`run` isn't a function: a table script defines `def run(t, p):`".into(), line: None, unit: None }]);
        };
        Ok(TableScript { source, takes_p, _module: module, function })
    }

    /// The script as written.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Runs the script on its table. What it gives back must be a table; anything it stops on is said with its line.
    pub fn run(&self, input: TableInput) -> Result<TableRan, Problem> {
        let notes: tables::Notes = Arc::default();
        let context = ScriptRun { lookups: input.lookups.clone(), log: None, tables: Some(tables::TableRun { work: input.work.clone(), tables: input.tables, read: Mutex::default(), notes: notes.clone(), checks: Mutex::default() }) };
        let said = Said::default();
        let mut table = Module::with_temp_heap(|module| -> Result<zd_core::Table, Problem> {
            // SAFETY: as `Formula::run`: the function was frozen when it compiled, and this module's frozen heap keeps a
            // reference to the heap it lives on for as long as the module, which outlives the call made here.
            let f = unsafe { self.function.owned_frozen_value(module.frozen_heap()) }.to_value();
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&said);
            eval.extra = Some(&context);
            let heap = eval.heap();
            let t = heap.alloc(tables::TableHandle { name: input.name.clone(), table: input.table.clone(), work: input.work.clone(), notes: notes.clone() });
            let result = if self.takes_p {
                let p = heap.alloc(Params { values: input.parameters.clone(), log: None, lookups: Some(input.lookups.clone()), reads: None });
                eval.eval_function(f, &[t, p], &[])
            } else {
                eval.eval_function(f, &[t], &[])
            };
            let given = result.map_err(|e| problem(&e, None))?;
            match given.downcast_ref::<tables::TableHandle>() {
                Some(h) => Ok((*h.table).clone()),
                None => Err(Problem {
                    message: format!("run gives back a table (return t, or one made from it), not {}", if given.is_none() { "nothing: is a return missing?".to_string() } else { given.get_type().to_string() }),
                    line: None,
                    unit: None,
                }),
            }
        })?;
        let read = context.tables.as_ref().map(|t| std::mem::take(&mut *t.read.lock().expect("read"))).unwrap_or_default();
        let notes = std::mem::take(&mut *notes.lock().expect("notes"));
        let asked = context.tables.as_ref().map(|t| std::mem::take(&mut *t.checks.lock().expect("checks"))).unwrap_or_default();
        let checks = checks::work_out(&mut table, &asked).map_err(|message| Problem { message, line: None, unit: None })?;
        Ok(TableRan { table, said: said.take(), notes, read, checks })
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
    fn call<'v>(&mut self, span: FileSpanRef, continued: bool, eval: &mut Evaluator<'v, '_, 'e>) -> starlark::Result<()> {
        // A statement resumed after a call it made is the same line running on: noted once.
        if continued {
            return Ok(());
        }
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
    if let Some(u) = v.downcast_ref::<alloc::AllocUnit>() {
        return u.to_string();
    }
    if v.downcast_ref::<Parts>().is_some() || v.downcast_ref::<Params>().is_some() || v.downcast_ref::<alloc::ColumnHandle>().is_some() {
        return v.to_string();
    }
    v.to_repr()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod table_tests;

#[cfg(test)]
mod alloc_tests;

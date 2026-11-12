//! `solid.interrogate`: the geology at each block (decision 0061, workstream 2). A block is a coal solid of one seam, named
//! by one of its attributes; each of that seam's grids is averaged over it by exact partial cells, as Vulcan does, and the
//! values are written onto the block as attributes, one a variable, named by its code. Its 2.5D column volume is written
//! beside them as a check (workstream 3). Nothing else: tonnes, moisture basis, dilution and run-of-mine coal are steps
//! after it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, Grid, Lattice, ObjectId, ObjectKind};
use zd_geom::{Variable, cells_in_solid, column_volume, columns_in_solid, composite, thickness_between};
use zd_io::{read_linked_grid, read_linked_surface};

use super::in_parallel_watched;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place};

/// The attribute that says a block has been interrogated, and against which model: what the run writes over is its own.
pub const INTERROGATED: &str = "interrogated";

/// The attribute a block's 2.5D column volume is written to, after any prefix (m³).
pub const COLUMN_VOLUME: &str = "column_volume";

/// Blocks listed by name in a note, before the rest are counted.
const NAMED: usize = 5;

pub struct Interrogate;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterrogateParams {
    /// The block solids.
    pub solids: Vec<ObjectId>,
    /// The seam model whose grids are averaged over them.
    pub model: ObjectId,
    /// The attribute whose value names each block's seam, matched to the model's horizon codes: the seam names its grids
    /// carry after any mine code (`seam1` of `grnseam1.asg`), which the surface stack writes onto the solids it makes; the
    /// name with the mine code in front matches too. `seam` unless given.
    #[serde(default = "seam_attribute_default")]
    pub seam_attribute: String,
    /// The variables to write, by code; every grid of each block's seam when none are given.
    #[serde(default)]
    pub variables: Vec<String>,
    /// What a variable is weighted by besides volume, by code. Unless given here, the model's density is by volume alone and
    /// every other variable by coal tonnes (volume × density).
    #[serde(default)]
    pub weights: BTreeMap<String, Vec<String>>,
    /// Put in front of each variable's code to name the attribute it's written to.
    #[serde(default)]
    pub prefix: String,
    /// A block with less of its volume than this between its seam's roof and floor is warned of: its seam attribute may be
    /// wrong. A share, 0 to 1.
    #[serde(default = "in_seam_default")]
    pub in_seam: f64,
    /// A block whose own volume differs from its 2.5D column volume by more than this share of it is warned of. 0.001 is
    /// 0.1 %.
    #[serde(default = "volume_tolerance_default")]
    pub volume_tolerance: f64,
}

fn volume_tolerance_default() -> f64 {
    0.001
}

fn seam_attribute_default() -> String {
    "seam".into()
}

fn in_seam_default() -> f64 {
    0.95
}

/// One grid read for the run.
struct Loaded {
    grid: Grid,
    changed: bool,
}

/// What a seam's blocks are averaged by: its variables in order, each with the places of its weights, and its roof and floor.
struct SeamGrids<'a> {
    codes: Vec<String>,
    grids: Vec<&'a [f64]>,
    weights: Vec<Vec<usize>>,
    roof: &'a [f64],
    floor: &'a [f64],
}

/// A block whose volume differs from its column: by what share, its name, id and middle, and the two volumes.
type Differs<'a> = (f64, &'a str, ObjectId, [f64; 3], f64, f64);

/// What the run found for one block.
struct BlockResult {
    values: Vec<(String, Option<f64>)>,
    /// Its share between its seam's roof and floor, where its nodes say.
    in_seam: Option<f64>,
    volume: f64,
    /// Its 2.5D column volume: its seam's thickness at each node over its plan in the node's cell.
    column: f64,
    /// Its own volume, the volume of record.
    own: f64,
}

impl Command for Interrogate {
    type Params = InterrogateParams;
    const NAME: &'static str = "solid.interrogate";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Average each block's seam grids over it and write the values onto it",
            inputs: vec![
                Field::new("solids", FieldType::Objects, "The block solids"),
                Field::object("model", ObjectKind::SeamModel, "The seam model whose grids are averaged over them"),
            ],
            params: vec![
                Field::new("seam_attribute", FieldType::Text, "The attribute naming each block's seam, matched to the model's horizon codes (seam)"),
                Field::new("variables", FieldType::Text, "Variables to write, by code (every grid of the seam when none)"),
                Field::new("weights", FieldType::Text, "What a variable is weighted by besides volume, by code (density by volume, the rest by coal tonnes)"),
                Field::new("prefix", FieldType::Text, "Put in front of each code to name its attribute"),
                Field::new("in_seam", FieldType::Number { unit: "" }, "Warn of a block with less than this share between its seam's roof and floor (0.95)"),
                Field::new("volume_tolerance", FieldType::Number { unit: "" }, "Warn of a block whose volume differs from its 2.5D column by more than this share (0.001)"),
            ],
            outputs: vec![Field::new("solids", FieldType::Objects, "The blocks, each with its values as attributes")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.solids.is_empty() {
            return Err(CommandError::Invalid("give the blocks to interrogate".into()));
        }
        if !(0.0..=1.0).contains(&p.in_seam) {
            return Err(CommandError::Invalid(format!("in_seam is a share from 0 to 1, not {}", p.in_seam)));
        }
        if !p.volume_tolerance.is_finite() || p.volume_tolerance < 0.0 {
            return Err(CommandError::Invalid(format!("volume_tolerance is a share of 0 or more, not {}", p.volume_tolerance)));
        }
        let model_object = inputs.object(p.model)?;
        let Body::SeamModel(model) = &model_object.body else {
            return Err(CommandError::WrongKind { id: p.model, expected: ObjectKind::SeamModel, found: model_object.body.kind() });
        };
        if !model.is_usable() {
            return Err(CommandError::Invalid(format!("'{}' has blocking issues in its stack: correct the grids and build it again first", model_object.name)));
        }
        let start = Instant::now();

        // Each block's seam, by its attribute.
        let mut blocks = Vec::new();
        let (mut no_seam, mut unknown): (Vec<String>, BTreeMap<String, Vec<String>>) = (Vec::new(), BTreeMap::new());
        for &id in &p.solids {
            let (object, solid) = inputs.solid(id)?;
            let Some(value) = object.attributes.get(&p.seam_attribute) else {
                no_seam.push(object.name.clone());
                continue;
            };
            let named = match value {
                AttrValue::Text(t) => t.trim().to_string(),
                AttrValue::Number(n) => format!("{n}"),
                AttrValue::Bool(b) => b.to_string(),
            };
            match model.horizons.iter().find(|h| names_seam(h, &named)) {
                Some(h) => blocks.push((object, solid, h.code.clone())),
                None => unknown.entry(named).or_default().push(object.name.clone()),
            }
        }
        let seams: BTreeSet<String> = blocks.iter().map(|(.., seam)| seam.clone()).collect();

        // The variables each seam writes and the grids each needs, its weights' included.
        let density = model.density.clone();
        let weights_of = |code: &str| -> Vec<String> {
            p.weights.get(code).cloned().unwrap_or_else(|| if code.eq_ignore_ascii_case(&density) { Vec::new() } else { vec![density.clone()] })
        };
        let mut wanted: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for seam in &seams {
            let horizon = model.horizon(seam).expect("matched above");
            let written: Vec<String> = if p.variables.is_empty() { horizon.grids.keys().cloned().collect() } else { p.variables.clone() };
            wanted.insert(seam.clone(), written);
        }
        // Names written are the run's own, or new: an attribute of the same name from elsewhere (Spry's, say) isn't written over.
        let mut names: BTreeSet<String> = wanted.values().flatten().map(|c| format!("{}{c}", p.prefix)).collect();
        names.insert(format!("{}{COLUMN_VOLUME}", p.prefix));
        if let Some((name, holder)) = blocks.iter().find_map(|(o, ..)| {
            (!o.attributes.contains_key(INTERROGATED)).then(|| names.iter().find(|n| o.attributes.contains_key(n.as_str())).map(|n| (n.clone(), o.name.clone()))).flatten()
        }) {
            return Err(CommandError::Invalid(format!(
                "'{holder}' already has an attribute '{name}' from elsewhere, which this would write over: give a prefix for the interrogated values"
            )));
        }

        // Each grid read once, in parallel, and checked against what the model was built from.
        let mut jobs: Vec<(String, String, std::path::PathBuf, zd_core::Fingerprint, bool)> = Vec::new();
        for (seam, written) in &wanted {
            let horizon = model.horizon(seam).expect("matched above");
            let mut codes: BTreeSet<String> = written.iter().cloned().collect();
            for code in written {
                codes.extend(weights_of(code));
            }
            for code in codes {
                if let Some(link) = horizon.grids.get(&code) {
                    jobs.push((seam.clone(), code, model.path(link), link.fingerprint, false));
                }
            }
            jobs.push((seam.clone(), "\u{0}roof".into(), model.path(&horizon.roof), horizon.roof.fingerprint, true));
            jobs.push((seam.clone(), "\u{0}floor".into(), model.path(&horizon.floor), horizon.floor.fingerprint, true));
        }
        let read = in_parallel_watched(inputs, "grids", &jobs, |(_, _, path, built, surface)| {
            let linked = if *surface { read_linked_surface(path) } else { read_linked_grid(path) };
            linked.map(|g| Loaded { changed: !g.fingerprint.same_content(built), grid: g.grid })
        })?;
        let mut loaded: HashMap<(String, String), Loaded> = HashMap::new();
        let (mut missing, mut off_lattice, mut changed) = (Vec::new(), Vec::new(), Vec::new());
        for ((seam, code, path, ..), result) in jobs.iter().zip(read) {
            match result {
                Err(e) => missing.push(format!("{} ({e})", path.display())),
                Ok(g) if Lattice::from(&g.grid) != model.lattice => off_lattice.push(path.display().to_string()),
                Ok(g) => {
                    if g.changed {
                        changed.push(path.display().to_string());
                    }
                    loaded.insert((seam.clone(), code.clone()), g);
                }
            }
        }
        if !missing.is_empty() || !off_lattice.is_empty() {
            let mut why = Vec::new();
            if !missing.is_empty() {
                why.push(format!("{} grids couldn't be read: {}", missing.len(), listed(&missing)));
            }
            if !off_lattice.is_empty() {
                why.push(format!("{} grids aren't on the model's lattice: {}", off_lattice.len(), listed(&off_lattice)));
            }
            return Err(CommandError::Invalid(format!("nothing was interrogated: {}", why.join("; "))));
        }

        // Each seam's variables as the geometry takes them.
        let seam_grids: HashMap<String, SeamGrids<'_>> = wanted
            .iter()
            .map(|(seam, written)| {
                let grid = |code: &str| loaded.get(&(seam.clone(), code.to_string())).map(|g| g.grid.z.as_slice());
                let mut codes: Vec<String> = written.iter().filter(|c| grid(c).is_some()).cloned().collect();
                // Weights not written are averaged too, to weigh by; only the written are kept.
                for code in written {
                    for w in weights_of(code) {
                        if grid(&w).is_some() && !codes.contains(&w) {
                            codes.push(w);
                        }
                    }
                }
                let weights = codes.iter().map(|c| weights_of(c).iter().map(|w| codes.iter().position(|x| x == w).unwrap_or(usize::MAX)).collect()).collect();
                let grids = codes.iter().map(|c| grid(c).expect("kept above")).collect();
                let surface = |which: &str| loaded[&(seam.clone(), format!("\u{0}{which}"))].grid.z.as_slice();
                (seam.clone(), SeamGrids { codes, grids, weights, roof: surface("roof"), floor: surface("floor") })
            })
            .collect();

        let lattice = model.lattice;
        let results = in_parallel_watched(inputs, "solids", &blocks, |(_, solid, seam)| {
            let g = &seam_grids[seam];
            let mesh = solid.mesh();
            // A weight with no grid on this seam takes every node out of what's weighted by it.
            let nan = vec![f64::NAN; lattice.cols * lattice.rows];
            let variables: Vec<Variable<'_>> = (0..g.codes.len())
                .map(|k| {
                    let weight_by: &[usize] = &g.weights[k];
                    if weight_by.contains(&usize::MAX) { Variable { grid: &nan, weight_by: &[] } } else { Variable { grid: g.grids[k], weight_by } }
                })
                .collect();
            let cells = cells_in_solid(mesh, &lattice);
            let averaged = composite(&cells, &variables);
            // The share between the seam's roof and floor, from the block's columns at the nodes it covers.
            let (mut inside, mut all) = (0.0, 0.0);
            for column in columns_in_solid(mesh, &lattice) {
                let t = column.thickness();
                all += t;
                inside += thickness_between(&column, g.floor[column.node], g.roof[column.node]).min(t);
            }
            BlockResult {
                values: g.codes.iter().cloned().zip(averaged.values).collect(),
                in_seam: (all > 0.0).then(|| inside / all),
                volume: averaged.volume,
                column: column_volume(&cells, g.floor, g.roof),
                own: solid.volume(),
            }
        })?;

        // The values onto the blocks.
        let mut outcome = Outcome::default();
        let mut no_value: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut outside: Vec<(f64, &str, ObjectId, [f64; 3])> = Vec::new();
        let mut differs: Vec<Differs<'_>> = Vec::new();
        let mut written_codes: BTreeSet<String> = BTreeSet::new();
        for ((object, solid, seam), result) in blocks.iter().zip(&results) {
            let keep: BTreeSet<&String> = wanted[seam].iter().collect();
            let mut attributes = object.attributes.clone();
            for (code, value) in &result.values {
                if !keep.contains(code) {
                    continue;
                }
                let name = format!("{}{code}", p.prefix);
                match value {
                    Some(v) => {
                        attributes.insert(name, AttrValue::Number(*v));
                        written_codes.insert(code.clone());
                    }
                    None => {
                        attributes.remove(&name);
                        no_value.entry(code.clone()).or_default().push(object.name.clone());
                    }
                }
            }
            for code in &wanted[seam] {
                if !result.values.iter().any(|(c, _)| c == code) {
                    attributes.remove(&format!("{}{code}", p.prefix));
                    no_value.entry(code.clone()).or_default().push(object.name.clone());
                }
            }
            attributes.insert(format!("{}{COLUMN_VOLUME}", p.prefix), AttrValue::Number(result.column));
            let off = if result.own > 0.0 { (result.column - result.own) / result.own } else { 0.0 };
            if off.abs() > p.volume_tolerance {
                let at = solid.mesh().bounds().map_or([0.0; 3], |b| b.centre());
                differs.push((off, object.name.as_str(), object.id, at, result.own, result.column));
            }
            attributes.insert(INTERROGATED.into(), AttrValue::Text(model_object.name.clone()));
            if let Some(share) = result.in_seam
                && share < p.in_seam
            {
                let at = solid.mesh().bounds().map_or([0.0; 3], |b| b.centre());
                outside.push((share, object.name.as_str(), object.id, at));
            }
            outcome.changes.push(Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None });
        }

        let volume: f64 = results.iter().map(|r| r.volume).sum();
        outcome.notes.push(Note::info(format!(
            "{} of {} blocks interrogated against '{}': {} seam{} ({}), {} variable{} written ({}), on {} m cells, {:.0} m³ in all, in {:.0} ms",
            blocks.len(),
            p.solids.len(),
            model_object.name,
            seams.len(),
            if seams.len() == 1 { "" } else { "s" },
            seams.iter().cloned().collect::<Vec<_>>().join(", "),
            written_codes.len(),
            if written_codes.len() == 1 { "" } else { "s" },
            written_codes.iter().map(|c| format!("{}{c}", p.prefix)).collect::<Vec<_>>().join(", "),
            lattice.cell_size[0],
            volume,
            start.elapsed().as_secs_f64() * 1e3
        )).on(model_object.id));
        if !no_seam.is_empty() {
            outcome.notes.push(
                Note::warning(Code::InterrogateNoSeam, format!("{} blocks have no '{}' attribute, so were left as they were: {}", no_seam.len(), p.seam_attribute, listed(&no_seam)))
                    .measured(no_seam.len() as f64, "blocks"),
            );
        }
        for (named, holders) in &unknown {
            outcome.notes.push(
                Note::warning(Code::InterrogateUnknownSeam, format!("{} blocks say their seam is '{named}', which isn't a horizon of '{}', so were left as they were: {}", holders.len(), model_object.name, listed(holders)))
                    .measured(holders.len() as f64, "blocks")
                    .on(model_object.id),
            );
        }
        for (code, holders) in &no_value {
            outcome.notes.push(
                Note::warning(Code::InterrogateNoValue, format!("{}{code}: no value over {} blocks (no grid for their seam, or none of its nodes under them), so none written: {}", p.prefix, holders.len(), listed(holders)))
                    .measured(holders.len() as f64, "blocks"),
            );
        }
        outside.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (share, name, id, at) in outside.iter().take(NAMED) {
            outcome.notes.push(
                Note::warning(Code::InterrogateOutsideSeam, format!("'{name}': only {:.0} % of it lies between its seam's roof and floor: its '{}' may be wrong", share * 100.0, p.seam_attribute))
                    .measured(share * 100.0, "%")
                    .at(Place::Point { at: *at })
                    .on(*id),
            );
        }
        if outside.len() > NAMED {
            outcome.notes.push(
                Note::warning(Code::InterrogateOutsideSeam, format!("{} more blocks lie less than {:.0} % in their seam", outside.len() - NAMED, p.in_seam * 100.0))
                    .measured((outside.len() - NAMED) as f64, "blocks"),
            );
        }
        let (own, column): (f64, f64) = results.iter().fold((0.0, 0.0), |(a, b), r| (a + r.own, b + r.column));
        outcome.notes.push(Note::info(format!(
            "Column check: {} of {} blocks within {} % of their 2.5D column volume, {:.0} m³ against {:.0} m³ in all ({:+.3} %)",
            blocks.len() - differs.len(),
            blocks.len(),
            p.volume_tolerance * 100.0,
            own,
            column,
            if own > 0.0 { (column - own) / own * 100.0 } else { 0.0 }
        )));
        differs.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
        for (off, name, id, at, own, column) in differs.iter().take(NAMED) {
            outcome.notes.push(
                Note::warning(Code::InterrogateColumnDiffers, format!("'{name}': {own:.0} m³ against a 2.5D column of {column:.0} m³ ({:+.2} %)", off * 100.0))
                    .measured(off * 100.0, "%")
                    .at(Place::Point { at: *at })
                    .on(*id),
            );
        }
        if differs.len() > NAMED {
            outcome.notes.push(
                Note::warning(Code::InterrogateColumnDiffers, format!("{} more blocks differ from their 2.5D column by more than {} %", differs.len() - NAMED, p.volume_tolerance * 100.0))
                    .measured((differs.len() - NAMED) as f64, "blocks"),
            );
        }
        if !changed.is_empty() {
            outcome.notes.push(
                Note::warning(Code::LinkChanged, format!("{} grid files differ from when '{}' was built, and were read as they are now: {}", changed.len(), model_object.name, listed(&changed)))
                    .measured(changed.len() as f64, "files")
                    .on(model_object.id),
            );
        }
        Ok(outcome)
    }
}

/// Whether a block's seam names this horizon: its code (`seam1`), or its grids' name with the mine code still in front
/// (`grnseam1`, from `grnseam1.srg`), case aside. Grids are named `<mine code><seam>.<variable>g`, and the definition takes
/// the mine code off to give the code.
pub fn names_seam(horizon: &zd_core::Horizon, named: &str) -> bool {
    let stem = |file: &str| {
        let name = std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        name.rsplit_once('.').map_or(name.clone(), |(stem, _)| stem.to_string())
    };
    horizon.code.eq_ignore_ascii_case(named) || stem(&horizon.roof.file).eq_ignore_ascii_case(named)
}

/// The first few, and how many more.
fn listed(items: &[String]) -> String {
    let mut out = items.iter().take(NAMED).map(|s| format!("'{s}'")).collect::<Vec<_>>().join(", ");
    if items.len() > NAMED {
        out.push_str(&format!(" and {} more", items.len() - NAMED));
    }
    out
}

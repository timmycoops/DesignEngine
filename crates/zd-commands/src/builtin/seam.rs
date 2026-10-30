use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, Grid, GridLink, Horizon, IssueKind, Lattice, ObjectKind, SeamModel, StackIssue};
use zd_geom::{StackHorizon, check_seam_stack, suggest_seam_tree};
use zd_io::{DefinedHorizon, SeamDefinition, read_linked_grid, read_seam_definition, read_vulcan_grid, vulcan_grid_name, write_seam_definition};

use super::in_parallel;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

fn default_tolerance() -> f64 {
    0.001
}

fn default_density() -> String {
    "rd".into()
}

/// `seam.suggest_definition`: writes a starting seam stack definition (CSV, decision 0007) for a
/// folder of Vulcan grids, to be checked and edited in Excel. Every seam with a roof (`sr`) and floor
/// (`sf`) on the folder's main lattice gets a row. Parents and order come from the grids' geometry
/// (which horizons sit inside which), never from the names; the working section is the horizons with
/// no daughters. The project is not changed.
pub struct SuggestSeamDefinition;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestSeamDefinitionParams {
    /// The folder holding the grids.
    pub dir: String,
    /// The project prefix on every file name, e.g. "pit". Blank: the longest start every roof grid's
    /// name shares, short of using up any name.
    #[serde(default)]
    pub prefix: String,
    /// The CSV file to write.
    pub path: String,
    /// Replace the file if it exists; by default an existing definition is never overwritten.
    #[serde(default)]
    pub overwrite: bool,
    /// Coal thinner than this (m) does not count when nesting horizons.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
}

impl Command for SuggestSeamDefinition {
    type Params = SuggestSeamDefinitionParams;
    const NAME: &'static str = "seam.suggest_definition";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Write a starting seam stack definition (CSV) for a folder of Vulcan grids",
            inputs: vec![],
            params: vec![
                Field::new("dir", FieldType::Path, "Folder of Vulcan grids"),
                Field::new("prefix", FieldType::Text, "Project prefix on the file names, e.g. pit; blank to work it out"),
                Field::new("path", FieldType::Path, "The definition CSV to write"),
                Field::new("overwrite", FieldType::Bool, "Replace an existing file"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "Coal thinner than this does not count when nesting horizons"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let start = Instant::now();
        let out = Path::new(&p.path);
        if out.exists() && !p.overwrite {
            return Err(CommandError::Invalid(format!("{} exists; set overwrite to replace it", p.path)));
        }
        let dir = Path::new(&p.dir);
        let mut files: Vec<(String, String, String)> = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|e| CommandError::Io(format!("{}: {e}", p.dir)))? {
            let entry = entry.map_err(|e| CommandError::Io(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some((stem, variable)) = vulcan_grid_name(&name) {
                files.push((stem, variable, name));
            }
        }
        let prefix = if p.prefix.is_empty() {
            shared_prefix(files.iter().filter(|(_, v, _)| v == "sr").map(|(stem, ..)| stem.as_str()))
        } else {
            p.prefix.to_ascii_lowercase()
        };
        let mut seams: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (stem, variable, name) in files {
            if let Some(seam) = stem.strip_prefix(&prefix)
                && !seam.is_empty()
            {
                seams.entry(seam.to_string()).or_default().insert(variable, name);
            }
        }
        seams.retain(|_, files| files.contains_key("sr") && files.contains_key("sf"));
        if seams.is_empty() {
            return Err(CommandError::Invalid(format!("no seams with both a roof (.srg) and a floor (.sfg) named {}*.srg in {}", p.prefix, p.dir)));
        }

        let codes: Vec<&String> = seams.keys().collect();
        let paths: Vec<PathBuf> = codes.iter().flat_map(|c| ["sr", "sf"].map(|v| dir.join(&seams[*c][v]))).collect();
        let read = in_parallel(&paths, |path| read_vulcan_grid(path).map(|g| g.grid));
        let mut messages = Vec::new();
        let mut usable: Vec<(usize, Grid, Grid)> = Vec::new();
        let mut pairs = read.into_iter();
        for (i, code) in codes.iter().enumerate() {
            match (pairs.next().expect("roof read"), pairs.next().expect("floor read")) {
                (Ok(roof), Ok(floor)) => usable.push((i, roof, floor)),
                (Err(e), _) | (_, Err(e)) => messages.push(format!("left out {code}: {e}")),
            }
        }
        // The main lattice is the one most roofs are on; seams on any other are left out.
        let mut counts: Vec<(Lattice, usize)> = Vec::new();
        for (_, roof, _) in &usable {
            let lattice = Lattice::from(roof);
            match counts.iter_mut().find(|(l, _)| *l == lattice) {
                Some((_, n)) => *n += 1,
                None => counts.push((lattice, 1)),
            }
        }
        let Some(&(main, _)) = counts.iter().max_by_key(|(_, n)| *n) else {
            return Err(CommandError::Invalid(format!("no seam's roof and floor could be read: {}", messages.join("; "))));
        };
        usable.retain(|(i, roof, floor)| {
            let same = Lattice::from(roof) == main && Lattice::from(floor) == main;
            if !same {
                messages.push(format!("left out {}: not on the main {} x {} lattice", codes[*i], main.cols, main.rows));
            }
            same
        });

        let grids: Vec<(&Grid, &Grid)> = usable.iter().map(|(_, r, f)| (r, f)).collect();
        let tree = suggest_seam_tree(&grids, p.tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let has_daughters = |i: usize| tree.iter().any(|s| s.parent == Some(i));
        let code_of = |i: usize| codes[usable[i].0].clone();
        let horizons: Vec<DefinedHorizon> = tree
            .iter()
            .map(|s| {
                let files = &seams[&code_of(s.index)];
                DefinedHorizon {
                    code: code_of(s.index),
                    parent: s.parent.map(code_of),
                    material: "coal".into(),
                    working: !has_daughters(s.index),
                    roof: files["sr"].clone(),
                    floor: files["sf"].clone(),
                    grids: files.iter().filter(|(v, _)| *v != "sr" && *v != "sf").map(|(v, f)| (v.clone(), f.clone())).collect(),
                }
            })
            .collect();
        let definition = SeamDefinition { folder: PathBuf::from(&p.dir), horizons };
        write_seam_definition(out, &definition).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;

        let mut outcome = Outcome::default().message(format!(
            "{} horizons (prefix '{prefix}') written to {} in {:.0} ms; check them, then build the model from it",
            definition.horizons.len(),
            p.path,
            start.elapsed().as_secs_f64() * 1e3
        ));
        for h in &definition.horizons {
            let parent_of = |code: &&String| definition.horizons.iter().find(|x| x.code == **code).and_then(|x| x.parent.as_ref());
            let depth = std::iter::successors(h.parent.as_ref(), parent_of).count();
            let role = if h.working { "working" } else { "parent" };
            outcome = outcome.message(format!("{}{} ({role}, {} other grids)", "  ".repeat(depth), h.code, h.grids.len()));
        }
        for m in messages {
            outcome = outcome.message(m);
        }
        Ok(outcome)
    }
}

/// `seam.build_model`: a seam model from a stack definition (CSV, decision 0007). Every grid it names
/// is read in place and fingerprinted, never copied; every grid must be on one lattice. The stack is
/// then checked, never changed: a floor above its roof, a working horizon crossing the one above, a
/// daughter outside its parent, and unpaired nulls are each listed with where they are. A model with
/// blocking issues is still made, so the issues can be seen, but solids and reserves refuse it until
/// the grids are fixed and the model is rebuilt.
pub struct BuildSeamModel;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSeamModelParams {
    /// The definition CSV.
    pub definition: String,
    pub layer: String,
    /// Defaults to the definition's file name.
    #[serde(default)]
    pub name: Option<String>,
    /// The variable code of the density grid (air-dried RD).
    #[serde(default = "default_density")]
    pub density: String,
    /// Overlaps up to this (m) are listed but do not block.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
}

impl Command for BuildSeamModel {
    type Params = BuildSeamModelParams;
    const NAME: &'static str = "seam.build_model";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Build and check a seam model from a stack definition",
            inputs: vec![],
            params: vec![
                Field::new("definition", FieldType::Path, "Seam stack definition (CSV)"),
                Field::new("layer", FieldType::Layer, "Layer for the model"),
                Field::new("name", FieldType::Text, "Optional; the definition's file name by default"),
                Field::new("density", FieldType::Text, "Variable code of the density grid, e.g. rd"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "Overlaps up to this are listed but do not block"),
            ],
            outputs: vec![Field::object("model", ObjectKind::SeamModel, "The checked seam model")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if !(p.tolerance.is_finite() && p.tolerance >= 0.0) {
            return Err(CommandError::Invalid("tolerance must be zero or more".into()));
        }
        let path = Path::new(&p.definition);
        let definition = read_seam_definition(path).map_err(|e| CommandError::Invalid(format!("{}: {e}", p.definition)))?;
        let density = p.density.to_ascii_lowercase();

        // Read every grid in place. Roofs and floors are kept for the checks; other grids are only
        // fingerprinted and lattice-checked, so a model with many quality grids stays small.
        let start = Instant::now();
        let jobs: Vec<(usize, String, String, bool)> = definition
            .horizons
            .iter()
            .enumerate()
            .flat_map(|(i, h)| {
                [("roof".to_string(), &h.roof, true), ("floor".to_string(), &h.floor, true)]
                    .into_iter()
                    .chain(h.grids.iter().map(|(v, f)| (v.clone(), f, false)))
                    .map(move |(v, f, keep)| (i, v, f.clone(), keep))
            })
            .collect();
        let read = in_parallel(&jobs, |(_, _, file, keep)| {
            read_linked_grid(&definition.path(file)).map(|g| (g.fingerprint, Lattice::from(&g.grid), keep.then_some(g.grid)))
        });
        let read_ms = start.elapsed().as_secs_f64() * 1e3;

        // The model's lattice is the first roof's that reads; every grid must be on it.
        let mut problems = Vec::new();
        let lattice = read.iter().find_map(|r| r.as_ref().ok().map(|(_, l, _)| *l));
        let mut links: Vec<BTreeMap<String, GridLink>> = vec![BTreeMap::new(); definition.horizons.len()];
        let mut grids: Vec<HashMap<String, Grid>> = vec![HashMap::new(); definition.horizons.len()];
        for ((h, variable, file, _), result) in jobs.iter().zip(read) {
            let code = &definition.horizons[*h].code;
            match result {
                Err(e) => problems.push(format!("horizon '{code}' {variable}: {file}: {e}")),
                Ok((_, l, _)) if Some(l) != lattice => problems.push(format!(
                    "horizon '{code}' {variable}: {file} is on a {} x {} lattice at {} m from ({}, {}), not the model's",
                    l.cols, l.rows, l.cell_size[0], l.origin[0], l.origin[1]
                )),
                Ok((fingerprint, _, grid)) => {
                    links[*h].insert(variable.clone(), GridLink { file: file.clone(), fingerprint });
                    if let Some(grid) = grid {
                        grids[*h].insert(variable.clone(), grid);
                    }
                }
            }
        }
        if !problems.is_empty() {
            return Err(CommandError::Invalid(format!("{} grid(s) cannot be used:\n{}", problems.len(), problems.join("\n"))));
        }
        let lattice = lattice.expect("a definition has at least one horizon, and every grid read");

        let index: HashMap<String, usize> = definition.horizons.iter().enumerate().map(|(i, h)| (h.code.to_ascii_lowercase(), i)).collect();
        let stack: Vec<StackHorizon<'_>> = definition
            .horizons
            .iter()
            .enumerate()
            .map(|(i, h)| StackHorizon {
                code: &h.code,
                parent: h.parent.as_ref().map(|p| index[&p.to_ascii_lowercase()]),
                working: h.working,
                roof: &grids[i]["roof"],
                floor: &grids[i]["floor"],
            })
            .collect();
        let check = Instant::now();
        let mut issues = check_seam_stack(&stack, p.tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let check_ms = check.elapsed().as_secs_f64() * 1e3;
        for h in definition.horizons.iter().filter(|h| h.working && !h.grids.contains_key(&density)) {
            issues.push(StackIssue { kind: IssueKind::NoDensity, horizon: h.code.clone(), other: None, nodes: 0, worst: 0.0, at: None, blocking: false });
        }

        let mut outcome = Outcome::default().message(format!(
            "{} horizons ({} working) on a {} x {} lattice at {} m; {} grids read in place and fingerprinted in {read_ms:.0} ms, stack checked in {check_ms:.0} ms",
            definition.horizons.len(),
            definition.horizons.iter().filter(|h| h.working).count(),
            lattice.cols,
            lattice.rows,
            lattice.cell_size[0],
            jobs.len()
        ));
        for (i, h) in definition.horizons.iter().enumerate() {
            outcome = outcome.message(describe_horizon(h, &grids[i]["roof"], &grids[i]["floor"], p.tolerance));
        }
        for issue in &issues {
            outcome = outcome.message(describe_issue(issue));
        }
        let blocking = issues.iter().filter(|i| i.blocking).count();
        outcome = outcome.message(if blocking == 0 {
            "No blocking issues: the stack can be used for solids and reserves".to_string()
        } else {
            format!("{blocking} blocking issue(s): solids and reserves will refuse this model until the grids are fixed and it is rebuilt")
        });

        let horizons = definition
            .horizons
            .iter()
            .zip(links)
            .map(|(h, mut links)| Horizon {
                code: h.code.clone(),
                parent: h.parent.clone(),
                material: h.material.clone(),
                working: h.working,
                roof: links.remove("roof").expect("roof linked"),
                floor: links.remove("floor").expect("floor linked"),
                grids: links,
            })
            .collect();
        let z_range = (0..definition.horizons.len())
            .filter(|&i| definition.horizons[i].working)
            .flat_map(|i| grids[i]["roof"].z_range().into_iter().chain(grids[i]["floor"].z_range()))
            .reduce(|[lo, hi], [a, b]| [lo.min(a), hi.max(b)]);
        let model =
            SeamModel { folder: definition.folder.to_string_lossy().into_owned(), density, tolerance: p.tolerance, lattice, z_range, horizons, issues };
        let name = p.name.clone().unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "seam model".into()));
        let mut done = Outcome::new(vec![Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::SeamModel(Arc::new(model)) }]);
        done.messages = outcome.messages;
        Ok(done)
    }
}

/// One line per horizon: where it sits in the stack and how much coal it holds.
fn describe_horizon(h: &DefinedHorizon, roof: &Grid, floor: &Grid, tolerance: f64) -> String {
    let thickness: Vec<f64> = roof.z.iter().zip(&floor.z).map(|(r, f)| r - f).filter(|t| *t > tolerance).collect();
    let place = match (&h.parent, h.working) {
        (Some(p), true) => format!("in {p}, working"),
        (Some(p), false) => format!("in {p}"),
        (None, true) => "working".into(),
        (None, false) => "not working".into(),
    };
    if thickness.is_empty() {
        return format!("{}: {place}; no coal thicker than {tolerance} m", h.code);
    }
    let max = thickness.iter().copied().fold(0.0, f64::max);
    let mean = thickness.iter().sum::<f64>() / thickness.len() as f64;
    format!("{}: {place}; {} at {} of {} nodes, up to {max:.2} m thick (mean {mean:.2} m)", h.code, h.material, thickness.len(), roof.z.len())
}

fn describe_issue(issue: &StackIssue) -> String {
    let severity = if issue.blocking { "BLOCKING" } else { "warning" };
    let other = issue.other.as_deref().unwrap_or("?");
    let what = match issue.kind {
        IssueKind::FloorAboveRoof => format!("{} floor above its roof", issue.horizon),
        IssueKind::Crossing => format!("{} roof above the floor of {other}", issue.horizon),
        IssueKind::OutsideParent => format!("{} outside its parent {other}", issue.horizon),
        IssueKind::Unpaired => format!("{} roof and floor not live at the same nodes", issue.horizon),
        IssueKind::NoDensity => return format!("{severity}: {} has no density grid; tonnes will need one", issue.horizon),
    };
    let at = issue.at.map(|[x, y]| format!(" at E {x:.1} N {y:.1}")).unwrap_or_default();
    if issue.worst > 0.0 {
        format!("{severity}: {what} at {} nodes, worst {:.3} m{at}", issue.nodes, issue.worst)
    } else {
        format!("{severity}: {what} at {} nodes, first{at}", issue.nodes)
    }
}

/// The longest start all the names share, kept at least one character short of the shortest name so
/// every seam keeps a code.
fn shared_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = names.next() else { return String::new() };
    let mut prefix: &str = first;
    let mut shortest = first.chars().count();
    let mut count = 1;
    for name in names {
        count += 1;
        let common = prefix.chars().zip(name.chars()).take_while(|(a, b)| a == b).count();
        prefix = &prefix[..prefix.char_indices().nth(common).map_or(prefix.len(), |(i, _)| i)];
        shortest = shortest.min(name.chars().count());
    }
    // One name alone says nothing about where its prefix ends.
    if count == 1 {
        return String::new();
    }
    prefix.chars().take(shortest.saturating_sub(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::shared_prefix;

    #[test]
    fn the_prefix_is_what_every_roof_name_shares() {
        let of = |names: &[&str]| shared_prefix(names.iter().copied());
        assert_eq!(of(&["mine2c", "mine2l", "minea1", "mineb3"]), "mine");
        assert_eq!(of(&["pitupper", "pitlower"]), "pit");
        // Never the whole of a name: "pitu" keeps the code "u".
        assert_eq!(of(&["pitu", "pituu", "pitul"]), "pit");
        assert_eq!(of(&["pitupper"]), "");
        assert_eq!(of(&[]), "");
    }
}

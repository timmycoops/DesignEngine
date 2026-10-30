//! The seam model: an explicit, ordered stack of horizons, each with roof and floor grids, a parent for
//! splits, and quality grids (decision 0007).
//!
//! The model links to its grid files and reads them in place; it never copies them into the project.
//! Each link keeps the file's fingerprint from when the model was built, so a changed file shows. The
//! stack is checked, never fixed: a floor above its roof, a horizon crossing the next one down, or a
//! daughter outside its parent is recorded as an issue with where it is, and is resolved in the source
//! model before solids or reserves use the stack.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Grid;

/// What a grid file looked like when it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub bytes: u64,
    pub modified_unix_ms: u64,
    /// 64-bit FNV-1a of the whole file.
    pub checksum: u64,
}

/// A grid file the model reads in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridLink {
    /// Relative to the model's folder, or absolute.
    pub file: String,
    pub fingerprint: Fingerprint,
}

/// The node lattice every grid of a model shares.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lattice {
    pub origin: [f64; 2],
    pub cell_size: [f64; 2],
    pub cols: usize,
    pub rows: usize,
}

impl From<&Grid> for Lattice {
    fn from(grid: &Grid) -> Self {
        Lattice { origin: grid.origin, cell_size: grid.cell_size, cols: grid.cols, rows: grid.rows }
    }
}

/// One horizon of the stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Horizon {
    /// The horizon's code, unique in the model (e.g. "4uu").
    pub code: String,
    /// The horizon this one splits from, if it is a daughter.
    pub parent: Option<String>,
    /// e.g. "coal".
    pub material: String,
    /// In the working section: the horizons stacked, in order, for solids and reserves.
    pub working: bool,
    pub roof: GridLink,
    pub floor: GridLink,
    /// Every other grid of the horizon by variable code: qualities, density, thicknesses.
    pub grids: BTreeMap<String, GridLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    /// The floor lies above the roof.
    FloorAboveRoof,
    /// In the working section, the roof rises above the floor of the next horizon up.
    Crossing,
    /// Where the daughter holds coal, it reaches above its parent's roof or below its parent's floor.
    OutsideParent,
    /// The roof is live where the floor is null, or the other way round.
    Unpaired,
    /// A working horizon with no density grid (a warning: volumes do not need it, tonnes will).
    NoDensity,
}

/// One problem found when the stack was checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackIssue {
    pub kind: IssueKind,
    pub horizon: String,
    /// The other horizon involved: the one above for a crossing, the parent for `OutsideParent`.
    pub other: Option<String>,
    /// Nodes affected.
    pub nodes: usize,
    /// The largest overlap in metres; zero where the issue is not a distance.
    pub worst: f64,
    /// Easting and northing of the worst node (the first one where there is no distance).
    pub at: Option<[f64; 2]>,
    /// Blocking issues stop solids and reserves using the model. Overlaps within the model's
    /// tolerance are listed but do not block.
    pub blocking: bool,
}

/// The seam model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeamModel {
    /// The folder the grid files are relative to.
    pub folder: String,
    /// The variable code of the density grid (air-dried RD).
    pub density: String,
    /// Overlaps up to this many metres are listed but do not block.
    pub tolerance: f64,
    pub lattice: Lattice,
    /// An RL that means the seam is absent, from the definition (decision 0020): see [`blank_no_seam`].
    #[serde(default)]
    pub no_seam: Option<f64>,
    /// Lowest floor and highest roof in the working section when the model was built, so it can be
    /// framed without reading its grids.
    #[serde(default)]
    pub z_range: Option<[f64; 2]>,
    /// Top of the stack first.
    pub horizons: Vec<Horizon>,
    pub issues: Vec<StackIssue>,
}

/// Blanks a horizon where its model marks it absent: a node with its roof or floor at `rl` holds no
/// seam, so both go null. RLs are compared in single precision, as the grids store them. Returns
/// the nodes blanked.
pub fn blank_no_seam(roof: &mut Grid, floor: &mut Grid, rl: f64) -> usize {
    let marked = |z: f64| z as f32 == rl as f32;
    let mut blanked = 0;
    for (r, f) in roof.z.iter_mut().zip(&mut floor.z) {
        if marked(*r) || marked(*f) {
            blanked += 1;
            *r = f64::NAN;
            *f = f64::NAN;
        }
    }
    blanked
}

impl SeamModel {
    pub fn horizon(&self, code: &str) -> Option<&Horizon> {
        self.horizons.iter().find(|h| h.code == code)
    }

    /// The horizons stacked for solids and reserves, top first.
    pub fn working_section(&self) -> impl Iterator<Item = &Horizon> {
        self.horizons.iter().filter(|h| h.working)
    }

    pub fn blocking_issues(&self) -> impl Iterator<Item = &StackIssue> {
        self.issues.iter().filter(|i| i.blocking)
    }

    /// True when nothing blocks solids and reserves using the stack.
    pub fn is_usable(&self) -> bool {
        self.blocking_issues().next().is_none()
    }

    /// Where a linked grid file is.
    pub fn path(&self, link: &GridLink) -> PathBuf {
        let file = Path::new(&link.file);
        if file.is_absolute() { file.to_path_buf() } else { Path::new(&self.folder).join(file) }
    }

    /// Every grid file the model links to, roof and floor first for each horizon.
    pub fn links(&self) -> impl Iterator<Item = (&Horizon, &str, &GridLink)> {
        self.horizons.iter().flat_map(|h| {
            [("roof", &h.roof), ("floor", &h.floor)].into_iter().chain(h.grids.iter().map(|(v, l)| (v.as_str(), l))).map(move |(v, l)| (h, v, l))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_at_the_no_seam_rl_goes_null_in_roof_and_floor_alike() {
        let lattice = |z: Vec<f64>| Grid::new([650_000.0, 7_450_000.0], [25.0, 25.0], 2, 2, z);
        // Coal at the first node, the roof marked at the second, both at the third, already null at the last.
        let mut roof = lattice(vec![100.0, 160.0, 160.0, f64::NAN]);
        let mut floor = lattice(vec![97.5, 90.0, 160.0, f64::NAN]);
        assert_eq!(blank_no_seam(&mut roof, &mut floor, 160.0), 2);
        assert_eq!((roof.z[0], floor.z[0]), (100.0, 97.5), "a real node is left exactly as it was");
        assert!(roof.z[1..].iter().chain(&floor.z[1..]).all(|z| z.is_nan()));
    }

    fn link(file: &str) -> GridLink {
        GridLink { file: file.into(), fingerprint: Fingerprint { bytes: 1, modified_unix_ms: 2, checksum: 3 } }
    }

    fn horizon(code: &str, parent: Option<&str>, working: bool) -> Horizon {
        let grids = BTreeMap::from([("rd".to_string(), link(&format!("{code}.rdg")))]);
        Horizon {
            code: code.into(),
            parent: parent.map(Into::into),
            material: "coal".into(),
            working,
            roof: link(&format!("{code}.srg")),
            floor: link(&format!("{code}.sfg")),
            grids,
        }
    }

    fn model() -> SeamModel {
        SeamModel {
            folder: "grids".into(),
            density: "rd".into(),
            tolerance: 0.001,
            lattice: Lattice { origin: [500_000.0, 7_000_000.0], cell_size: [10.0, 10.0], cols: 3, rows: 2 },
            no_seam: None,
            z_range: Some([90.0, 100.0]),
            horizons: vec![horizon("a", None, false), horizon("au", Some("a"), true), horizon("al", Some("a"), true)],
            issues: vec![],
        }
    }

    #[test]
    fn working_section_and_links() {
        let m = model();
        assert_eq!(m.working_section().map(|h| h.code.as_str()).collect::<Vec<_>>(), ["au", "al"]);
        let links: Vec<(&str, &str)> = m.links().map(|(h, v, _)| (h.code.as_str(), v)).collect();
        assert_eq!(links[..3], [("a", "roof"), ("a", "floor"), ("a", "rd")]);
        assert_eq!(m.path(&m.horizons[0].roof), Path::new("grids").join("a.srg"));
        assert_eq!(m.horizon("al").unwrap().parent.as_deref(), Some("a"));
    }

    #[test]
    fn only_blocking_issues_stop_use() {
        let mut m = model();
        assert!(m.is_usable());
        let issue = |blocking| StackIssue {
            kind: IssueKind::Crossing,
            horizon: "al".into(),
            other: Some("au".into()),
            nodes: 3,
            worst: 0.5,
            at: Some([500_010.0, 7_000_000.0]),
            blocking,
        };
        m.issues.push(issue(false));
        assert!(m.is_usable());
        m.issues.push(issue(true));
        assert!(!m.is_usable());
        assert_eq!(m.blocking_issues().count(), 1);
    }

    #[test]
    fn round_trips_through_json_exactly() {
        let mut m = model();
        m.tolerance = 0.1 + 0.2;
        m.lattice.origin = [612_345.678_901_234_5, 7_548_765.432_109_876];
        let back: SeamModel = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}

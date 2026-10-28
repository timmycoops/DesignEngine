//! Check 2: build seam solids between roof and floor, then cut them with a design surface
//! (closed into a half-space beyond the solid's bounds) using Manifold.
//!
//! `cargo bench -p bench --bench check2_booleans` runs synthetic pits of 250, 500 and 1000 cells
//! a side at 10 m, plus every seam found under `testdata/golden/`. Set `ZD_CHECK2_SIZES` (e.g.
//! `250,2000`) to change the synthetic sizes and `ZD_GOLDEN_CELL` (default 5 m) for the lattice
//! real roof and floor triangulations are sampled onto. Writes `bench/out/check2.md`.

use std::fmt::Write as _;

use bench::golden::{GoldenPit, discover_pits, load_triangulation};
use bench::synth::{synth_design, synth_pit};
use bench::{count, out_dir, timed};
use zd_core::{Grid, TriMesh};
use zd_geom::{SeamSolid, build_seam_solid, cut_solid_by_surface, sample_surface_on_grid, triangulate_grid};

/// Phase 0 target: "seconds, not minutes, per cut".
const CUT_TARGET_MS: f64 = 10_000.0;
/// Below + above must equal the input to this relative tolerance.
const CONSERVATION_TOLERANCE: f64 = 1e-6;
/// Seam cells thinner than this are left out of the solid.
const MIN_THICKNESS: f64 = 0.1;

struct Row {
    case: String,
    build_ms: f64,
    seam: SeamSolid,
    design_tris: usize,
    outcome: Result<zd_geom::CutResult, String>,
}

fn main() {
    let sizes: Vec<usize> = std::env::var("ZD_CHECK2_SIZES")
        .unwrap_or_else(|_| "250,500,1000".into())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let mut rows = Vec::new();
    for cells in sizes {
        let cell = 10.0;
        let pit = synth_pit(cells, cell);
        let design = triangulate_grid(&synth_design(cells, cell));
        let (seam, build_ms) = timed(|| build_seam_solid(&pit.roof, &pit.floor, MIN_THICKNESS).expect("synthetic seam"));
        eprintln!("synthetic {cells}x{cells}: solid {} triangles, cutting...", count(seam.mesh.triangle_count()));
        let outcome = cut_solid_by_surface(&seam.mesh, &design).map_err(|e| e.to_string());
        rows.push(Row {
            case: format!("synthetic {cells}x{cells} @ {cell} m"),
            build_ms,
            seam,
            design_tris: design.triangle_count(),
            outcome,
        });
    }

    let golden_cell: f64 = std::env::var("ZD_GOLDEN_CELL").ok().and_then(|s| s.parse().ok()).unwrap_or(5.0);
    for pit in discover_pits() {
        rows.extend(golden_rows(&pit, golden_cell));
    }

    let report = render(&rows);
    println!("{report}");
    let path = out_dir().join("check2.md");
    std::fs::write(&path, &report).expect("write check2.md");
    eprintln!("wrote {}", path.display());
}

fn golden_rows(pit: &GoldenPit, cell: f64) -> Vec<Row> {
    let Some(design_path) = &pit.design else {
        eprintln!("{}: no design, bow or topo surface to cut with; skipped", pit.name);
        return Vec::new();
    };
    let design = match load_triangulation(design_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", pit.name);
            return Vec::new();
        }
    };
    let mut rows = Vec::new();
    for seam in &pit.seams {
        let loaded = load_triangulation(&seam.roof).and_then(|r| load_triangulation(&seam.floor).map(|f| (r, f)));
        let (roof, floor) = match loaded {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("{} {}: {e}", pit.name, seam.name);
                continue;
            }
        };
        let Some(lattice) = common_lattice(&roof, &floor, cell) else {
            eprintln!("{} {}: roof and floor do not overlap in plan", pit.name, seam.name);
            continue;
        };
        let roof_grid = sample_surface_on_grid(&roof, &lattice);
        let floor_grid = sample_surface_on_grid(&floor, &lattice);
        let (built, build_ms) = timed(|| build_seam_solid(&roof_grid.grid, &floor_grid.grid, MIN_THICKNESS));
        let seam_solid = match built {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{} {}: {e}", pit.name, seam.name);
                continue;
            }
        };
        let outcome = cut_solid_by_surface(&seam_solid.mesh, &design).map_err(|e| e.to_string());
        rows.push(Row {
            case: format!("{} {} @ {cell} m (cut by {})", pit.name, seam.name, file_name(design_path)),
            build_ms,
            seam: seam_solid,
            design_tris: design.triangle_count(),
            outcome,
        });
    }
    rows
}

/// A lattice of `cell` metres over the plan overlap of two surfaces.
fn common_lattice(a: &TriMesh, b: &TriMesh, cell: f64) -> Option<Grid> {
    let (ba, bb) = (a.bounds()?, b.bounds()?);
    let x0 = (ba.min[0].max(bb.min[0]) / cell).ceil() * cell;
    let y0 = (ba.min[1].max(bb.min[1]) / cell).ceil() * cell;
    let x1 = ba.max[0].min(bb.max[0]);
    let y1 = ba.max[1].min(bb.max[1]);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let cols = ((x1 - x0) / cell).floor() as usize + 1;
    let rows = ((y1 - y0) / cell).floor() as usize + 1;
    Some(Grid::new([x0, y0], [cell, cell], cols, rows, vec![f64::NAN; cols * rows]))
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn render(rows: &[Row]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "### Check 2: seam solid cut by a design surface (Manifold)\n");
    let _ = writeln!(
        out,
        "Pass = both parts watertight (closed, manifold, consistently wound), both volumes > 0, \
         below + above = input within {CONSERVATION_TOLERANCE:e} relative, and cut under {} s.\n",
        CUT_TARGET_MS / 1e3
    );
    let _ = writeln!(
        out,
        "| Case | Solid tris | Design tris | Half-space tris | Build solid ms | Close ms | To Manifold ms | Boolean ms | From Manifold ms | **Cut total ms** | Below tris | Above tris | Watertight | Below m3 | Above m3 | Conservation err | Column vs mesh | Result |"
    );
    let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|---|");
    for row in rows {
        let solid_tris = count(row.seam.mesh.triangle_count());
        let column_diff = (row.seam.column_volume - row.seam.mesh.signed_volume()).abs();
        match &row.outcome {
            Ok(cut) => {
                let t = cut.timings;
                let watertight = cut.below_validity.is_watertight() && cut.above_validity.is_watertight();
                let conservation = ((cut.below_volume + cut.above_volume) - cut.input_volume).abs() / cut.input_volume;
                let pass = watertight
                    && cut.below_volume > 0.0
                    && cut.above_volume > 0.0
                    && conservation < CONSERVATION_TOLERANCE
                    && t.total_ms() < CUT_TARGET_MS;
                let _ = writeln!(
                    out,
                    "| {} | {solid_tris} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | **{:.0}** | {} | {} | {} | {:.1} | {:.1} | {:.1e} | {:.1e} m3 | {} |",
                    row.case,
                    count(row.design_tris),
                    count(cut.halfspace_triangles),
                    row.build_ms,
                    t.close_ms,
                    t.to_manifold_ms,
                    t.boolean_ms,
                    t.from_manifold_ms,
                    t.total_ms(),
                    count(cut.below.triangle_count()),
                    count(cut.above.triangle_count()),
                    if watertight { "yes" } else { "**no**" },
                    cut.below_volume,
                    cut.above_volume,
                    conservation,
                    column_diff,
                    if pass { "PASS" } else { "**FAIL**" },
                );
                if !cut.design_covers_solid {
                    let _ = writeln!(out, "|  ↳ design surface does not cover the solid in plan; uncovered coal reports as above | | | | | | | | | | | | | | | | | |");
                }
            }
            Err(e) => {
                let _ = writeln!(out, "| {} | {solid_tris} | {} | | {:.0} | | | | | | | | | | | | | **FAIL**: {e} |", row.case, count(row.design_tris), row.build_ms);
            }
        }
    }
    let _ = writeln!(out, "\nSeam solid logs (cells excluded or changed while building):\n");
    for row in rows {
        let log = &row.seam.log;
        let _ = writeln!(
            out,
            "- {}: {} active cells; {} thin or null cells left out; {} corner-pinch cells removed; {} floor nodes clamped to the roof.",
            row.case,
            count(log.active_cells),
            count(log.thin_or_null_cells),
            count(log.pinch_cells_removed),
            count(log.floor_nodes_clamped)
        );
    }
    out
}

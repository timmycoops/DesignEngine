//! Benches in the column engine (decision 0071, point 8, Spry's way): a horizon's benches joined into one unit with each
//! bench's in situ share of its volume, worked on whole, and put back on the benches by those shares at a write.
//!
//! A made-up block, pushed out of order:
//!
//! | horizon | stack | bench | material | m³    | m   | RD  | ash |
//! |---------|-------|-------|----------|-------|-----|-----|-----|
//! | OB      | 0     | 110   | waste    | 4000  | 4   | 2.3 | 85  |
//! | OB      | 0     | 100   | waste    | 6000  | 6   | 2.4 | 80  |
//! | cu      | 1     | 98    | coal     | 1000  | 1   | 1.4 | 10  |
//! | cu      | 1     | 96    | coal     | 3000  | 3   | 1.5 | 12  |
//! | cu      | 1     | 94    | coal     | 2000  | 2   | 1.6 | 14  |
//! | cl      | 2     | (not benched) | coal | 300 | 3 | 1.5 | 20 |
//!
//! Joined, cu is 6,000 m³ and 6 m, RD by volume 9,100 / 6,000 and ash by tonnes 112,800 / 9,100, its benches' shares
//! 1/6, 1/2 and 1/3.

use std::sync::Arc;
use std::time::Instant;

use zd_reserves::{AddUp, Amount, Became, BecameKind, Bench, Column, ColumnError, ColumnKey, End, Field, FieldKind, Fields, Labels, Origin};

/// Shares multiply and sums add in another order, so a joined or shared-out sum matches its parts to within this.
const TOLERANCE: f64 = 1e-12;

const VOLUME: usize = 0;
const THICKNESS: usize = 1;
const RD: usize = 3;
const ASH: usize = 4;

fn fields() -> Arc<Fields> {
    Arc::new(
        Fields::new(vec![
            Field::new("volume", FieldKind::Volume, AddUp::Sum),
            Field::new("thickness", FieldKind::Thickness, AddUp::Sum),
            Field::new("area", FieldKind::Area, AddUp::Max),
            Field::new("rd", FieldKind::Density, AddUp::Volume),
            Field::new("ash", FieldKind::Quality, AddUp::Mass { density: "rd".into() }),
        ])
        .unwrap(),
    )
}

type Row = (&'static str, u32, Option<(&'static str, f64)>, &'static str, &'static str, [f64; 5]);

const ROWS: [Row; 6] = [
    ("cl", 2, None, "coal", "cl", [300.0, 3.0, 100.0, 1.5, 20.0]),
    ("cu 94", 1, Some(("94", 94.0)), "coal", "cu", [2000.0, 2.0, 1000.0, 1.6, 14.0]),
    ("OB 100", 0, Some(("100", 100.0)), "waste", "cu", [6000.0, 6.0, 1000.0, 2.4, 80.0]),
    ("cu 98", 1, Some(("98", 98.0)), "coal", "cu", [1000.0, 1.0, 1000.0, 1.4, 10.0]),
    ("OB 110", 0, Some(("110", 110.0)), "waste", "cu", [4000.0, 4.0, 1000.0, 2.3, 85.0]),
    ("cu 96", 1, Some(("96", 96.0)), "coal", "cu", [3000.0, 3.0, 1000.0, 1.5, 12.0]),
];

fn column() -> Column {
    let mut c = Column::new(ColumnKey { case: "Base".into(), strip: "5".into(), block: "B1".into() }, fields());
    for (name, stack, bench, material, seam, values) in ROWS {
        let origin = Origin { object: format!("solid-{name}"), name: format!("5_B1_{name}") };
        let bench = bench.map(|(label, floor)| Bench::new(label, floor));
        c.push_bench_input(origin, stack, Labels::new(material, seam), bench, values.to_vec()).unwrap();
    }
    c.sort();
    c
}

fn names(c: &Column) -> Vec<&str> {
    c.units().iter().map(|u| c.origins()[u.origin as usize].name.as_str()).collect()
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1.0)
}

fn assert_close(a: f64, b: f64, what: &str) {
    assert!(close(a, b, TOLERANCE), "{what}: {a} against {b}");
}

fn inputs_summed() -> [f64; 2] {
    let v: f64 = ROWS.iter().map(|r| r.5[VOLUME]).sum();
    let t: f64 = ROWS.iter().map(|r| r.5[THICKNESS]).sum();
    [v, t]
}

#[test]
fn the_sort_puts_a_horizons_benches_top_down() {
    let c = column();
    assert_eq!(names(&c), ["5_B1_OB 110", "5_B1_OB 100", "5_B1_cu 98", "5_B1_cu 96", "5_B1_cu 94", "5_B1_cl"]);
}

#[test]
fn ground_below_and_above_the_levels_sorts_with_its_benches() {
    let mut c = Column::new(ColumnKey::default(), fields());
    for (label, floor) in [("below 0", f64::NEG_INFINITY), ("490", 490.0), ("above 500", 500.0), ("0", 0.0)] {
        let origin = Origin { object: label.into(), name: label.into() };
        c.push_bench_input(origin, 0, Labels::new("waste", "cu"), Some(Bench::new(label, floor)), vec![1.0, 1.0, 1.0, 2.3, 85.0]).unwrap();
    }
    c.sort();
    assert_eq!(names(&c), ["above 500", "490", "0", "below 0"]);
}

#[test]
fn joining_makes_a_unit_a_horizon_with_each_bench_s_share() {
    let mut c = column();
    assert_eq!(c.join_benches(), 2);
    assert_eq!(c.len(), 3);
    let [ob, cu, cl] = [&c.units()[0], &c.units()[1], &c.units()[2]];
    assert_eq!((ob.stack, cu.stack, cl.stack), (0, 1, 2));
    assert_eq!((ob.joined, cu.joined, cl.joined), (Some(0), Some(1), None));
    assert!(ob.bench.is_none() && cu.bench.is_none() && cl.bench.is_none());

    // cu: 6,000 m³, 6 m, RD by volume, ash by tonnes, the largest area.
    assert_close(cu.value(VOLUME), 6000.0, "cu volume");
    assert_close(cu.value(THICKNESS), 6.0, "cu thickness");
    assert_close(cu.value(RD), 9100.0 / 6000.0, "cu RD");
    assert_close(cu.value(ASH), 112_800.0 / 9100.0, "cu ash");
    assert_close(cu.value(2), 1000.0, "cu area");
    // OB: 10,000 m³, RD (9,200 + 14,400) / 10,000, ash (9,200 × 85 + 14,400 × 80) / 23,600.
    assert_close(ob.value(VOLUME), 10_000.0, "OB volume");
    assert_close(ob.value(RD), 23_600.0 / 10_000.0, "OB RD");
    assert_close(ob.value(ASH), (9200.0 * 85.0 + 14_400.0 * 80.0) / 23_600.0, "OB ash");

    let join = &c.joins()[1];
    assert_eq!(join.stack, 1);
    assert_eq!(join.in_situ, Labels::new("coal", "cu"));
    let labels: Vec<&str> = join.benches.iter().map(|b| b.bench.label.as_str()).collect();
    assert_eq!(labels, ["98", "96", "94"]);
    for (b, share) in join.benches.iter().zip([1.0 / 6.0, 0.5, 1.0 / 3.0]) {
        assert_close(b.share, share, "cu share");
    }
    // The in situ values stay with the join, for the in situ rows written by bench.
    assert_eq!(join.benches[1].values, ROWS[5].5.to_vec());
    assert_eq!(c.origins()[join.benches[2].origin as usize].name, "5_B1_cu 94");

    // Nothing is lost by joining: the summed fields balance to the inputs.
    let balance = c.balance();
    let [v, t] = inputs_summed();
    assert_close(balance[VOLUME], v, "balance volume");
    assert_close(balance[THICKNESS], t, "balance thickness");
}

#[test]
fn a_write_puts_a_loss_and_the_rest_back_on_the_benches_by_share() {
    let mut c = column();
    c.join_benches();
    // Half a metre of roof loss off the joined cu: 500 m³ of its 6,000.
    let split = c.split(1, Amount::Thickness(0.5), End::Top).unwrap();
    c.label_became(split.piece, Became { kind: BecameKind::Loss, detail: "roof".into() });
    let rest = split.rest.unwrap();
    assert_eq!(c.units()[split.piece].joined, Some(1), "a piece split from a joined unit keeps its join");

    let rows = c.by_bench([rest]);
    let labels: Vec<&str> = rows.iter().map(|r| r.bench.as_ref().unwrap().label.as_str()).collect();
    assert_eq!(labels, ["98", "96", "94"]);
    let rd = 9100.0 / 6000.0;
    for (row, share) in rows.iter().zip([1.0 / 6.0, 0.5, 1.0 / 3.0]) {
        assert_close(row.values[VOLUME], 5500.0 * share, "ROM volume by bench");
        assert_close(row.values[THICKNESS], 5.5 * share, "ROM thickness by bench");
        assert_close(row.values[RD], rd, "ROM RD by bench, the joined unit's");
        assert_close(row.values[ASH], 112_800.0 / 9100.0, "ROM ash by bench, the joined unit's");
        assert_eq!(row.units, [c.units()[rest].id]);
    }
    let loss = c.by_bench([split.piece]);
    let lost: f64 = loss.iter().map(|r| r.values[VOLUME]).sum();
    assert_close(lost, 500.0, "loss by bench");
    assert_close(loss[1].values[VOLUME], 250.0, "bench 96's half of the loss");
}

#[test]
fn dilution_goes_back_on_its_own_horizon_s_benches() {
    let mut c = column();
    c.join_benches();
    // 0.2 m of OB floor taken into cu's ROM: 200 m³ of OB's 10,000, shared 0.4 and 0.6 over its benches.
    let split = c.split(0, Amount::Thickness(0.2), End::Bottom).unwrap();
    c.label_became(split.piece, Became { kind: BecameKind::Dilution, detail: "roof".into() });
    let rest = c.position(c.units()[1].id).unwrap();
    assert_eq!(c.units()[rest].in_situ.seam, "cu");
    let rows = c.by_bench([split.piece, rest + 1]);
    let labels: Vec<&str> = rows.iter().map(|r| r.bench.as_ref().unwrap().label.as_str()).collect();
    assert_eq!(labels, ["110", "100", "98", "96", "94"]);
    assert_close(rows[0].values[VOLUME], 80.0, "dilution on bench 110");
    assert_close(rows[1].values[VOLUME], 120.0, "dilution on bench 100");
    assert_close(rows[3].values[VOLUME], 3000.0, "cu on bench 96");
    assert_close(rows[0].values[RD], 23_600.0 / 10_000.0, "dilution keeps OB's RD");
}

#[test]
fn rows_by_bench_add_up_to_the_units_and_unbenched_units_come_last() {
    let mut c = column();
    let before = c.by_bench(0..c.len());
    // Before joining, each bench is its input exactly.
    assert_eq!(before.len(), 6);
    assert_eq!(before[2].bench.as_ref().unwrap().label, "98");
    assert_eq!(before[2].values, ROWS[3].5.to_vec());
    assert!(before[5].bench.is_none(), "cl, not benched, is last");
    assert_eq!(before[5].values, ROWS[0].5.to_vec());

    c.join_benches();
    let after = c.by_bench(0..c.len());
    assert_eq!(after.len(), 6);
    for (a, b) in after.iter().zip(&before) {
        assert_eq!(a.bench, b.bench);
        // Each bench back by its share: the volume and thickness of a bench as input, the qualities its horizon's.
        assert_close(a.values[VOLUME], b.values[VOLUME], "volume back on its bench");
        assert_close(a.values[THICKNESS], b.values[THICKNESS], "thickness back on its bench");
    }
    let total: f64 = after.iter().map(|r| r.values[VOLUME]).sum();
    assert_close(total, inputs_summed()[0], "rows add up to the column");
}

#[test]
fn a_lone_bench_or_one_already_worked_isn_t_joined() {
    let mut c = Column::new(ColumnKey::default(), fields());
    let lone = Origin { object: "a".into(), name: "a".into() };
    c.push_bench_input(lone, 0, Labels::new("waste", "cu"), Some(Bench::new("100", 100.0)), vec![10.0, 1.0, 1.0, 2.3, 85.0]).unwrap();
    for (label, floor) in [("98", 98.0), ("96", 96.0)] {
        let origin = Origin { object: label.into(), name: label.into() };
        c.push_bench_input(origin, 1, Labels::new("coal", "cu"), Some(Bench::new(label, floor)), vec![10.0, 1.0, 1.0, 1.4, 10.0]).unwrap();
    }
    c.sort();
    c.label_became(2, Became { kind: BecameKind::SmallVolume, detail: String::new() });
    assert_eq!(c.join_benches(), 0);
    assert_eq!(c.len(), 3);
    assert_eq!(c.units()[0].bench.as_ref().unwrap().label, "100");
    let rows = c.by_bench([0]);
    assert_eq!(rows[0].bench.as_ref().unwrap().label, "100");
}

#[test]
fn shares_fall_back_to_thickness_then_equal() {
    let mut c = Column::new(ColumnKey::default(), fields());
    for (label, floor, thickness) in [("98", 98.0, 1.0), ("96", 96.0, 3.0)] {
        let origin = Origin { object: label.into(), name: label.into() };
        c.push_bench_input(origin, 0, Labels::new("coal", "cu"), Some(Bench::new(label, floor)), vec![0.0, thickness, 0.0, 1.4, 10.0]).unwrap();
    }
    for (label, floor) in [("98", 98.0), ("96", 96.0), ("94", 94.0)] {
        let origin = Origin { object: label.into(), name: label.into() };
        c.push_bench_input(origin, 1, Labels::new("coal", "cl"), Some(Bench::new(label, floor)), vec![0.0, 0.0, 0.0, 1.5, 20.0]).unwrap();
    }
    c.sort();
    assert_eq!(c.join_benches(), 2);
    let by_thickness: Vec<f64> = c.joins()[0].benches.iter().map(|b| b.share).collect();
    assert_eq!(by_thickness, [0.25, 0.75]);
    let equal: Vec<f64> = c.joins()[1].benches.iter().map(|b| b.share).collect();
    assert_eq!(equal, [1.0 / 3.0; 3]);
}

#[test]
fn a_bench_with_no_rl_is_refused() {
    let mut c = Column::new(ColumnKey::default(), fields());
    let origin = Origin { object: "x".into(), name: "5_B1_cu x".into() };
    let err = c.push_bench_input(origin, 0, Labels::new("coal", "cu"), Some(Bench::new("x", f64::NAN)), vec![1.0, 1.0, 1.0, 1.4, 10.0]);
    assert_eq!(err, Err(ColumnError::BadBench { unit: "5_B1_cu x".into(), bench: "x".into() }));
}

/// The demo pit benched every 10 m: 90 block columns, 1,815 units, at most 23 a column, about six horizons a column (a
/// count of its blocks, `testdata/synthetic/workflow_demo.py`). Here 90 columns of six horizons of four benches each,
/// 2,160 units, joined, a roof loss split off each horizon and every unit written by bench.
#[test]
fn demo_pit_sized_columns_join_and_write_by_bench_quickly() {
    let fields = fields();
    let started = Instant::now();
    let mut rows = 0;
    let mut units = 0;
    for k in 0..90 {
        let mut c = Column::new(ColumnKey { case: "Base".into(), strip: (k / 10).to_string(), block: format!("B{}", k % 10) }, fields.clone());
        for stack in (0..6u32).rev() {
            for b in 0..4 {
                let floor = 100.0 - 10.0 * f64::from(stack * 4 + b);
                let origin = Origin { object: format!("{k}-{stack}-{b}"), name: format!("{k}-{stack}-{b}") };
                let material = if stack % 2 == 0 { "waste" } else { "coal" };
                let values = vec![1000.0 + f64::from(b), 10.0, 100.0, 1.5 + 0.1 * f64::from(b), 10.0 + f64::from(stack)];
                c.push_bench_input(origin, stack, Labels::new(material, &stack.to_string()), Some(Bench::new(&floor.to_string(), floor)), values)
                    .unwrap();
                units += 1;
            }
        }
        c.sort();
        let input = c.balance();
        assert_eq!(c.join_benches(), 6);
        for at in (0..c.len()).rev() {
            let split = c.split(at, Amount::Thickness(0.5), End::Top).unwrap();
            c.label_became(split.piece, Became { kind: BecameKind::Loss, detail: "roof".into() });
        }
        let written = c.by_bench(0..c.len());
        rows += written.len();
        let volume: f64 = written.iter().map(|r| r.values[VOLUME]).sum();
        assert!(close(volume, input[VOLUME], TOLERANCE));
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("90 columns, {units} bench units joined, split and written as {rows} rows by bench: {ms:.1} ms");
    assert_eq!(rows, 90 * 24);
}

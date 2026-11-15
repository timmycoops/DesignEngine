//! The column engine on a made-up block with known answers (decision 0070): two seams, cu over cl, a parting between,
//! overburden above and waste below.
//!
//! | horizon | stack | material | seam | m³    | m   | RD  | ash | min parting | ash cut-off |
//! |---------|-------|----------|------|-------|-----|-----|-----|-------------|-------------|
//! | OB      | 0     | waste    | cu   | 1000  | 10  | 2.3 | 85  |             |             |
//! | cu      | 1     | coal     | cu   | 200   | 2   | 1.4 | 10  | 0.3         | 30          |
//! | cu-cl   | 2     | waste    | cl   | 50    | 0.5 | 2.2 | 80  |             |             |
//! | cl      | 3     | coal     | cl   | 300   | 3   | 1.5 | 20  | 0.5         | 25          |
//! | below   | 4     | waste    |      | 2000  | 20  | 2.3 | 85  |             |             |
//!
//! The waste above a seam carries that seam's name, as Spry's stack does.

use std::sync::Arc;

use zd_reserves::{AddUp, Amount, Became, BecameKind, Column, ColumnError, ColumnKey, End, Field, FieldKind, Fields, Labels, Origin};

/// Summed fields share out by multiplication and subtraction, so a split's pieces add back to within this, relative.
const BALANCE_TOLERANCE: f64 = 1e-12;

fn fields() -> Arc<Fields> {
    Arc::new(
        Fields::new(vec![
            Field::new("volume", FieldKind::Volume, AddUp::Sum),
            Field::new("thickness", FieldKind::Thickness, AddUp::Sum),
            Field::new("area", FieldKind::Area, AddUp::Max),
            Field::new("rd", FieldKind::Density, AddUp::Volume),
            Field::new("ash", FieldKind::Quality, AddUp::Mass { density: "rd".into() }),
            Field::new("min_parting", FieldKind::Setting, AddUp::Max),
            Field::new("ash_cutoff", FieldKind::Setting, AddUp::Min),
        ])
        .unwrap(),
    )
}

const N: f64 = f64::NAN;

/// The block's units, pushed bottom up so the sort has work to do.
fn column() -> Column {
    let mut c = Column::new(ColumnKey::new("Base", &["5", "B1"]), fields());
    let rows: [(&str, u32, &str, &str, [f64; 7]); 5] = [
        ("below cl", 4, "waste", "", [2000.0, 20.0, 100.0, 2.3, 85.0, N, N]),
        ("cl", 3, "coal", "cl", [300.0, 3.0, 100.0, 1.5, 20.0, 0.5, 25.0]),
        ("cu-cl", 2, "waste", "cl", [50.0, 0.5, 100.0, 2.2, 80.0, N, N]),
        ("cu", 1, "coal", "cu", [200.0, 2.0, 100.0, 1.4, 10.0, 0.3, 30.0]),
        ("OB", 0, "waste", "cu", [1000.0, 10.0, 100.0, 2.3, 85.0, N, N]),
    ];
    for (name, stack, material, seam, values) in rows {
        let origin = Origin { object: format!("solid-{name}"), name: format!("5_B1_{name}") };
        c.push_input(origin, stack, Labels::new(material, seam), values.to_vec()).unwrap();
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

#[test]
fn the_sort_puts_the_column_in_the_seam_models_order() {
    let c = column();
    assert_eq!(names(&c), ["5_B1_OB", "5_B1_cu", "5_B1_cu-cl", "5_B1_cl", "5_B1_below cl"]);
    assert_eq!(c.units().iter().map(|u| u.stack).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
}

#[test]
fn a_section_adds_up_by_each_fields_rule() {
    let c = column();
    let f = c.fields();
    let section = c.add_up(1..4);
    assert_eq!(section[f.at("volume").unwrap()], 550.0);
    assert_eq!(section[f.at("thickness").unwrap()], 5.5);
    assert_eq!(section[f.at("area").unwrap()], 100.0);
    // RD by volume: (200 × 1.4 + 50 × 2.2 + 300 × 1.5) / 550 = 840 / 550.
    assert!(close(section[f.at("rd").unwrap()], 840.0 / 550.0, 1e-15));
    // Ash by tonnes: (280 × 10 + 110 × 80 + 450 × 20) / 840 = 20,600 / 840.
    assert!(close(section[f.at("ash").unwrap()], 20_600.0 / 840.0, 1e-15));
    // Settings: the strictest parting and the lowest cut-off of the plies that have one; the parting has none.
    let coal = c.add_up([1, 3]);
    assert_eq!(coal[f.at("min_parting").unwrap()], 0.5);
    assert_eq!(coal[f.at("ash_cutoff").unwrap()], 25.0);
    assert!(section[f.at("min_parting").unwrap()].is_nan(), "a ply with no setting is missing, never passed over");
}

#[test]
fn a_missing_value_with_weight_is_missing_and_one_without_weight_is_not() {
    let mut c = column();
    let (ash, volume) = (c.fields().at("ash").unwrap(), c.fields().volume());
    c.unit_mut(2).set(ash, f64::NAN);
    assert!(c.add_up(1..4)[ash].is_nan());
    c.unit_mut(2).set(volume, 0.0);
    assert!(close(c.add_up(1..4)[ash], (2800.0 + 9000.0) / 730.0, 1e-15));
    assert!(c.add_up([]).iter().enumerate().all(|(i, v)| if c.fields().is_summed(i) { *v == 0.0 } else { v.is_nan() }));
}

#[test]
fn a_split_from_the_top_sits_above_the_rest_and_shares_out_the_summed_fields() {
    let mut c = column();
    let f = c.fields().clone();
    let cu = c.units()[1].id;
    // A roof loss of 0.1 m off cu.
    let split = c.split(1, Amount::Thickness(0.1), End::Top).unwrap();
    assert_eq!((split.piece, split.rest), (1, Some(2)));
    let (piece, rest) = (&c.units()[1], &c.units()[2]);
    assert_eq!(rest.id, cu, "the rest keeps the unit's id");
    assert_ne!(piece.id, cu);
    assert_eq!(piece.origin, rest.origin, "the piece keeps where it came from");
    assert_eq!(piece.in_situ, rest.in_situ);
    assert_eq!(piece.value(f.thickness()), 0.1);
    assert!(close(piece.value(f.volume()), 10.0, 1e-15));
    assert!(close(rest.value(f.volume()), 190.0, 1e-15));
    assert!(close(rest.value(f.thickness()), 1.9, 1e-15));
    for name in ["area", "rd", "ash", "min_parting", "ash_cutoff"] {
        let i = f.at(name).unwrap();
        assert_eq!(piece.value(i).to_bits(), rest.value(i).to_bits(), "{name} is kept on both pieces");
    }
}

#[test]
fn a_split_from_the_bottom_sits_below_the_rest() {
    let mut c = column();
    let volume = c.fields().volume();
    // Floor dilution for cu: 5 m³ off the top of the parting; then a 0.2 m floor loss off cl.
    let split = c.split(2, Amount::Volume(5.0), End::Top).unwrap();
    assert_eq!((split.piece, split.rest), (2, Some(3)));
    assert_eq!(c.units()[2].value(volume), 5.0);
    assert_eq!(c.units()[3].value(volume), 45.0);
    let split = c.split(4, Amount::Thickness(0.2), End::Bottom).unwrap();
    assert_eq!((split.piece, split.rest), (5, Some(4)));
    assert!(close(c.units()[5].value(volume), 20.0, 1e-15));
    assert_eq!(names(&c), ["5_B1_OB", "5_B1_cu", "5_B1_cu-cl", "5_B1_cu-cl", "5_B1_cl", "5_B1_cl", "5_B1_below cl"]);
    // The sort keeps the pieces of one place in their order.
    let ids: Vec<u32> = c.units().iter().map(|u| u.id).collect();
    c.sort();
    assert_eq!(c.units().iter().map(|u| u.id).collect::<Vec<_>>(), ids);
}

#[test]
fn the_column_balances_to_its_inputs_through_splits_and_set_asides() {
    let mut c = column();
    let before = c.balance();
    c.split(1, Amount::Thickness(0.1), End::Top).unwrap();
    c.label_became(1, Became { kind: BecameKind::Loss, detail: "roof".into() });
    c.split(3, Amount::Fraction(0.3), End::Top).unwrap();
    c.label_became(3, Became { kind: BecameKind::Dilution, detail: "floor".into() });
    c.split(5, Amount::Thickness(0.2), End::Bottom).unwrap();
    c.split(5, Amount::Volume(17.0), End::Top).unwrap();
    assert_eq!(c.len(), 9);
    let after = c.balance();
    for (i, field) in c.fields().list().iter().enumerate() {
        if c.fields().is_summed(i) {
            assert!(close(after[i], before[i], BALANCE_TOLERANCE), "{}: {} against {}", field.name, after[i], before[i]);
        }
    }
    assert!(c.units()[1].is_set_aside());
    assert!(!c.units()[3].is_set_aside(), "dilution goes into the ROM");
    assert_eq!(c.units().iter().filter(|u| u.is_set_aside()).count(), 1);
}

#[test]
fn asking_for_more_than_a_unit_holds_is_a_shortfall_said_in_full() {
    let mut c = column();
    let cu = c.units()[1].id;
    assert_eq!(c.split(1, Amount::Volume(400.0), End::Top), Err(ColumnError::ShortOf { unit: cu, asked: 400.0, had: 200.0 }));
    assert_eq!(c.split(1, Amount::Thickness(0.0), End::Top), Err(ColumnError::BadAmount { unit: cu, asked: 0.0 }));
    assert!(matches!(c.split(1, Amount::Fraction(f64::NAN), End::Top), Err(ColumnError::BadAmount { .. })));
    assert_eq!(c.len(), 5, "a refused split changes nothing");
    // All of it is the whole unit, with nothing left; a hair over is rounding, not a shortfall.
    assert_eq!(c.split(1, Amount::Volume(200.0 * (1.0 + 1e-14)), End::Top).unwrap().rest, None);
    assert_eq!(c.len(), 5);
}

#[test]
fn runs_group_neighbours_the_way_spry_groups_the_waste_above_a_seam_with_it() {
    let c = column();
    let runs = c.runs(|a, b| a.in_situ.seam == b.in_situ.seam);
    assert_eq!(runs, [0..2, 2..4, 4..5]);
    assert!(c.runs(|_, _| false).iter().all(|r| r.len() == 1));
    assert_eq!(Column::new(ColumnKey::default(), fields()).runs(|_, _| true), []);
}

#[test]
fn fields_are_checked_once() {
    let v = || Field::new("volume", FieldKind::Volume, AddUp::Sum);
    let t = || Field::new("thickness", FieldKind::Thickness, AddUp::Sum);
    assert_eq!(Fields::new(vec![v(), t(), v()]).unwrap_err(), ColumnError::DuplicateField("volume".into()));
    assert_eq!(Fields::new(vec![t()]).unwrap_err(), ColumnError::NeedsOne { kind: "volume".into(), found: 0 });
    let averaged = Field::new("thickness", FieldKind::Thickness, AddUp::Volume);
    assert_eq!(Fields::new(vec![v(), averaged]).unwrap_err(), ColumnError::NotSummed("thickness".into()));
    let ash = |d: &str| Field::new("ash", FieldKind::Quality, AddUp::Mass { density: d.into() });
    assert!(matches!(Fields::new(vec![v(), t(), ash("rd")]), Err(ColumnError::NoDensity { .. })));
    assert!(matches!(Fields::new(vec![v(), t(), ash("ash")]), Err(ColumnError::NoDensity { .. })));
    let mut c = Column::new(ColumnKey::default(), fields());
    let short = c.push_input(Origin::default(), 0, Labels::default(), vec![1.0]);
    assert!(matches!(short, Err(ColumnError::Values { expected: 7, got: 1, .. })));
    let negative = c.push_input(Origin::default(), 0, Labels::default(), vec![-1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    assert!(matches!(negative, Err(ColumnError::BadSize { .. })));
}

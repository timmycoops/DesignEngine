//! Parameter tables and dataflows through the registry, as the app and Python call them (decision 0072): saved from rows,
//! CSV text or a CSV file, revised and undone, written back out as CSV; a dataflow naming its tables, warned where a
//! setting has no column in any of them.

use serde_json::json;
use zd_commands::{Code, CommandError, RunRecord, Session};
use zd_core::{AttrValue, Attributes, Body, ChangeKind, ObjectId, ParamValue};

const CSV: &str = "when seam,when strip,ash_cutoff (%),waste_rd (t/m3),loader\nU,1-5,30,,\n\"U, M\",,,2.4,\n,,35,2.3,EX3600\n";

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn table(s: &Session, id: ObjectId) -> zd_core::ParameterTable {
    match &s.project().object(id).expect("live").body {
        Body::ParameterTable(t) => (**t).clone(),
        _ => panic!("a parameter table"),
    }
}

fn attrs(pairs: &[(&str, AttrValue)]) -> Attributes {
    let mut a = Attributes::new();
    for (k, v) in pairs {
        a.insert(k.to_string(), v.clone());
    }
    a
}

#[test]
fn a_parameter_table_saved_from_csv_is_revised_and_undone() {
    let mut s = Session::with_builtins();
    let record = s.run("parameters.save", json!({ "name": "PIT B assumptions", "csv": CSV })).unwrap();
    let id = created(&record)[0];
    assert_eq!(record.messages[0], "saved PIT B assumptions: 3 rows, 3 values, filters on seam, strip");
    let object = s.project().object(id).unwrap();
    assert_eq!((object.layer, object.revision), (s.project().layer_named("parameters").unwrap().id, 1));
    let t = table(&s, id);
    let u3 = attrs(&[("seam", AttrValue::Text("U".into())), ("strip", AttrValue::Number(3.0))]);
    assert_eq!(t.lookup(&u3, "ash_cutoff"), Some((0, &ParamValue::Number(30.0))));
    assert_eq!(t.lookup(&u3, "waste_rd"), Some((1, &ParamValue::Number(2.4))));

    // A row changed, saved back as rows: the next revision, one undo.
    let mut changed = t.clone();
    changed.rows[0].values.insert("ash_cutoff".into(), ParamValue::Number(28.0));
    s.run("parameters.save", json!({ "table": id, "name": "PIT B assumptions", "content": changed })).unwrap();
    assert_eq!(s.project().object(id).unwrap().revision, 2);
    assert_eq!(table(&s, id).lookup(&u3, "ash_cutoff"), Some((0, &ParamValue::Number(28.0))));
    s.undo();
    assert_eq!(table(&s, id), t);
}

#[test]
fn a_parameter_table_goes_out_and_back_as_a_csv_file() {
    let mut s = Session::with_builtins();
    let id = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": CSV })).unwrap())[0];
    let path = std::env::temp_dir().join(format!("zd-parameters-{}.csv", std::process::id()));
    s.run("parameters.export_csv", json!({ "table": id, "path": path })).unwrap();
    let back = created(&s.run("parameters.save", json!({ "name": "read back", "path": path })).unwrap())[0];
    assert_eq!(table(&s, back), table(&s, id));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_table_given_two_ways_or_that_doesn_t_hold_together_is_refused() {
    let mut s = Session::with_builtins();
    let err = s.run("parameters.save", json!({ "name": "x", "csv": CSV, "path": "a.csv" })).unwrap_err();
    assert!(matches!(err, CommandError::Invalid(ref m) if m.contains("one way")), "{err}");
    let err = s.run("parameters.save", json!({ "name": "x", "csv": "when seam,seam\nU,1\n" })).unwrap_err();
    assert!(err.to_string().contains("named twice"), "{err}");
}

fn dataflow(parameters: &[ObjectId]) -> serde_json::Value {
    json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "as" },
            { "name": "ash_cutoff", "kind": "setting", "add_up": { "by": "min" }, "from": "parameter", "column": "ash_cutoff" },
            { "name": "min_parting", "kind": "setting", "add_up": { "by": "max" }, "from": "parameter", "column": "min_parting" },
        ],
        "steps": [{ "step": "initialise" }, { "step": "in_situ" }],
        "parameters": parameters,
    })
}

#[test]
fn a_dataflow_names_its_tables_and_a_setting_none_of_them_gives_is_said() {
    let mut s = Session::with_builtins();
    let id = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": CSV })).unwrap())[0];
    let record = s.run("dataflow.save", json!({ "name": "Reserves", "content": dataflow(&[id]) })).unwrap();
    let flow = created(&record)[0];
    assert_eq!(record.messages[0], "saved Reserves: 7 fields, 2 of 2 steps on, 1 parameter table");
    let warned: Vec<&str> = record.notes.iter().filter(|n| n.code == Some(Code::DataflowNoColumn)).map(|n| n.text.as_str()).collect();
    assert_eq!(warned, ["min_parting comes from the parameter column min_parting, which none of its 1 table has"]);
    let Body::Dataflow(d) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    assert_eq!(d.parameters, [id]);

    // Revised, with a step turned off: the next revision.
    let mut content = dataflow(&[id]);
    content["steps"][1]["on"] = json!(false);
    let record = s.run("dataflow.save", json!({ "dataflow": flow, "name": "Reserves", "content": content })).unwrap();
    assert_eq!(record.messages[0], "saved Reserves: 7 fields, 1 of 2 steps on, 1 parameter table");
    assert_eq!(s.project().object(flow).unwrap().revision, 2);
}

#[test]
fn a_dataflow_that_doesn_t_hold_together_or_names_no_table_is_refused() {
    let mut s = Session::with_builtins();
    let mut content = dataflow(&[]);
    content["fields"].as_array_mut().unwrap().remove(1);
    let err = s.run("dataflow.save", json!({ "name": "x", "content": content })).unwrap_err();
    assert!(err.to_string().contains("thickness"), "{err}");
    // A solid where a parameter table should be.
    let solid = created(
        &s.run(
            "solid.from_mesh",
            json!({ "name": "box", "layer": "solids", "vertices": [[0.0,0.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0],[1.0,0.0,1.0],[1.0,1.0,1.0],[0.0,1.0,1.0]],
                "triangles": [[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]] }),
        )
        .unwrap(),
    )[0];
    let err = s.run("dataflow.save", json!({ "name": "x", "content": dataflow(&[solid]) })).unwrap_err();
    assert!(matches!(err, CommandError::WrongKind { .. }), "{err}");
}

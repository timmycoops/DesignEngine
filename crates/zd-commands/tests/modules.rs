//! Commands by module, and a run refused when its module is off (decision 0098). One test, in its own file: the
//! modules on are the process's, so nothing else may run beside it.

use std::io::Read;

use serde_json::json;
use zd_commands::modules::{is_on, modules_on, set_every_module_on, set_licence_stamp, set_modules_on};
use zd_commands::{Code, CommandError, Module, Registry, Session};

#[test]
fn a_command_runs_only_while_its_module_is_on() {
    // Every command names a module, and the Python module has none (the bindings check it themselves).
    let registry = Registry::with_builtins();
    let modules = registry.modules();
    assert_eq!(modules.len(), registry.specs().len());
    assert!(modules.values().all(|m| *m != Module::Python));
    assert_eq!(modules["string.create"], Module::Design);
    assert_eq!(modules["solid.cut_by_seams"], Module::Solids);
    assert_eq!(modules["reserves.run"], Module::Reserves);
    assert_eq!(modules["export.dxf"], Module::Viewer);
    for module in Module::ALL {
        assert_eq!(Module::from_name(module.name()), Some(module));
    }

    // Until a host says, every module is on.
    assert!(Module::ALL.into_iter().all(is_on));

    // A licence with no modules: the Viewer is still on.
    set_modules_on([]);
    assert_eq!(modules_on().into_iter().collect::<Vec<_>>(), vec![Module::Viewer]);

    let mut s = Session::with_builtins();
    s.run("layer.create", json!({ "name": "lines" })).expect("a Viewer command runs");
    let points = json!([[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let refused = s.run("string.create", json!({ "name": "toe", "layer": "lines", "points": points })).unwrap_err();
    assert_eq!(refused, CommandError::NotLicensed { command: "string.create".into(), module: Module::Design });
    assert_eq!(refused.code(), Code::RefusedNotLicensed);
    assert!(refused.to_string().contains("Design"), "{refused}");
    assert_eq!(s.project().objects().count(), 0, "a refused run changes nothing");

    // Design on: the same run goes through.
    set_modules_on([Module::Design]);
    s.run("string.create", json!({ "name": "toe", "layer": "lines", "points": points })).expect("Design is on");
    assert_eq!(s.project().objects().count(), 1);

    set_every_module_on();
    assert!(Module::ALL.into_iter().all(is_on));

    // The licence the copy runs under is written in the project's manifest; none, nothing is.
    let dir = std::env::temp_dir().join(format!("zd-licence-stamp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = |s: &mut Session, name: &str| -> serde_json::Value {
        let path = dir.join(name);
        s.save(&path).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut text = String::new();
        zip.by_name("manifest.json").unwrap().read_to_string(&mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    assert_eq!(manifest(&mut s, "none.zdp").get("licence"), None);
    set_licence_stamp(Some("ZD-0001, licensed to Example Coal Pty Ltd".into()));
    assert_eq!(manifest(&mut s, "stamped.zdp")["licence"], "ZD-0001, licensed to Example Coal Pty Ltd");
    set_licence_stamp(None);
    let _ = std::fs::remove_dir_all(&dir);
}

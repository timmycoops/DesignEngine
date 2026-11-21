//! Commands by module, and a run refused when its module is off (decision 0098). One test, in its own file: the
//! modules on are the process's, so nothing else may run beside it.

use serde_json::json;
use zd_commands::modules::{is_on, modules_on, set_every_module_on, set_modules_on};
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
}

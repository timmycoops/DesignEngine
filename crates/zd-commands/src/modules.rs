//! Which part of the app each command belongs to, and which parts are on (decision 0098).
//!
//! The engine knows only this: every command names its module where it's registered, and a run of a command whose
//! module is off is refused before it reads anything. Which modules are on is the host's to say (the app and the Python
//! bindings read a licence; the engine never does). Until a host says, every module is on, as it is for the engine's
//! own tests and scripts.

use std::collections::BTreeSet;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// A part of the app a licence turns on. The Viewer is always on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    /// Opening, showing, measuring, interrogating and comparing, importing and exporting: nothing designed or cut.
    Viewer,
    /// Strings, faces, ramps, surfaces and editing objects.
    Design,
    /// Solids, cuts, blocks, benches and seam models.
    Solids,
    /// Parameters, dataflows and their formulas, reserves runs.
    Reserves,
    /// The Python bindings (`import zd`), checked by the bindings themselves; no command belongs to it.
    Python,
}

impl Module {
    pub const ALL: [Module; 5] = [Module::Viewer, Module::Design, Module::Solids, Module::Reserves, Module::Python];

    /// The name a module is written as in a licence, such as `solids`.
    pub fn name(self) -> &'static str {
        match self {
            Module::Viewer => "viewer",
            Module::Design => "design",
            Module::Solids => "solids",
            Module::Reserves => "reserves",
            Module::Python => "python",
        }
    }

    /// The name a planner reads, such as `Solids and blocks`.
    pub fn title(self) -> &'static str {
        match self {
            Module::Viewer => "Viewer",
            Module::Design => "Design",
            Module::Solids => "Solids and blocks",
            Module::Reserves => "Reserves",
            Module::Python => "Python",
        }
    }

    pub fn from_name(name: &str) -> Option<Module> {
        Module::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// `None` until a host says: every module on.
static ON: RwLock<Option<BTreeSet<Module>>> = RwLock::new(None);

/// Turns on these modules and only these, for every session in this process; the Viewer is always on.
pub fn set_modules_on(modules: impl IntoIterator<Item = Module>) {
    let mut on: BTreeSet<Module> = modules.into_iter().collect();
    on.insert(Module::Viewer);
    *ON.write().unwrap_or_else(|e| e.into_inner()) = Some(on);
}

/// Every module on again, as before any host said.
pub fn set_every_module_on() {
    *ON.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The modules on now.
pub fn modules_on() -> BTreeSet<Module> {
    ON.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| Module::ALL.into_iter().collect())
}

pub fn is_on(module: Module) -> bool {
    ON.read().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|on| on.contains(&module))
}

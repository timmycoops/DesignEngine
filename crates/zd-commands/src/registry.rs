use std::collections::BTreeMap;

use crate::command::DynCommand;
use crate::{Command, CommandSpec, Module, builtin};

/// Every command, by name, with the module it belongs to (decision 0098).
#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<&'static str, (Module, Box<dyn DynCommand>)>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Registry::default()
    }

    /// The engine's own commands.
    pub fn with_builtins() -> Self {
        let mut registry = Registry::new();
        builtin::register_all(&mut registry);
        registry
    }

    /// Adds a command, in the module a licence turns on to run it. Names must be unique.
    pub fn register<C: Command>(&mut self, module: Module, command: C) {
        let previous = self.commands.insert(C::NAME, (module, Box::new(command)));
        assert!(previous.is_none(), "command '{}' registered twice", C::NAME);
    }

    /// Specs of every command, sorted by name.
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.commands.values().map(|(_, c)| c.spec()).collect()
    }

    /// The module a command belongs to.
    pub fn module_of(&self, name: &str) -> Option<Module> {
        self.commands.get(name).map(|(module, _)| *module)
    }

    /// Every command's module, by name.
    pub fn modules(&self) -> BTreeMap<&'static str, Module> {
        self.commands.iter().map(|(name, (module, _))| (*name, *module)).collect()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&dyn DynCommand> {
        self.commands.get(name).map(|(_, c)| c.as_ref())
    }
}

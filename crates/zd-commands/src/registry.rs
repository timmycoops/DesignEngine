use std::collections::BTreeMap;

use crate::command::DynCommand;
use crate::{Command, CommandSpec, builtin};

/// Every command, by name.
#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<&'static str, Box<dyn DynCommand>>,
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

    /// Adds a command. Names must be unique.
    pub fn register<C: Command>(&mut self, command: C) {
        let previous = self.commands.insert(C::NAME, Box::new(command));
        assert!(previous.is_none(), "command '{}' registered twice", C::NAME);
    }

    /// Specs of every command, sorted by name.
    pub fn specs(&self) -> Vec<CommandSpec> {
        self.commands.values().map(|c| c.spec()).collect()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&dyn DynCommand> {
        self.commands.get(name).map(|c| c.as_ref())
    }
}

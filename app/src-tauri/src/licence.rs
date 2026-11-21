//! The licence this copy runs under (decision 0098): read as the app starts and when one is loaded, and handed to the
//! engine as the modules on and the stamp for reports and projects. The engine never reads a licence itself.

use zd_commands::Module;
use zd_commands::modules::{set_licence_stamp, set_modules_on};
use zd_licence::Status;

/// Turns on the modules a licence turns on, and sets the stamp.
pub fn apply(status: &Status) {
    set_modules_on(status.modules.iter().filter_map(|m| Module::from_name(m)));
    set_licence_stamp(status.stamp());
}

/// Reads this machine's licence and applies it.
pub fn check() -> Status {
    let status = zd_licence::check_this_machine();
    apply(&status);
    status
}

/// Where this copy's licence stands: who it's licensed to, its modules and dates, and this machine's code.
#[tauri::command]
pub fn licence_status() -> Status {
    check()
}

/// Puts a licence file on this machine, once it's one this version trusts, and applies it.
#[tauri::command]
pub fn load_licence(path: String) -> Result<Status, String> {
    let status = zd_licence::install(std::path::Path::new(&path))?;
    apply(&status);
    Ok(status)
}

/// Every command's module by name, for the tool list to say what a tool needs.
#[tauri::command]
pub fn command_modules() -> std::collections::BTreeMap<&'static str, Module> {
    zd_commands::Registry::with_builtins().modules()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_licence_names_the_engines_modules() {
        let engine: Vec<&str> = Module::ALL.iter().map(|m| m.name()).collect();
        assert_eq!(engine, zd_licence::MODULES);
    }
}

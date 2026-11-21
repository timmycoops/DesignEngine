//! `zdlic`: issue and read Zovlyn licence files (decision 0098). Run by Zovlyn on its own machine, with the signing key
//! kept off the repo and off any build machine.
//!
//! ```text
//! zdlic machine                            this machine's code
//! zdlic keygen <number> <key file>         a new signing key; prints its public key for TRUSTED in zd-licence
//! zdlic issue --key <key file> --licence ZD-0001 --to "Example Coal Pty Ltd" --modules design,solids,reserves,python
//!             [--machine M-XXXX-XXXX-XXXX]... [--until 2027-06-30] [--updates-until 2027-06-30] --out <file.zdlic>
//! zdlic show <file.zdlic>                  what a licence says, and whether this build trusts it
//! ```
//! `--modules all` lists every module; with no `--machine` the licence runs on any machine.

use std::path::PathBuf;
use std::process::ExitCode;

use zd_licence::{Day, FORMAT, KeyFile, Keys, Kind, Licence, MODULES, machine_code, new_key, read, sign};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("zdlic: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("machine") => {
            println!("{}", machine_code());
            Ok(())
        }
        Some("keygen") => keygen(&args[1..]),
        Some("issue") => issue(&args[1..]),
        Some("show") => show(&args[1..]),
        _ => Err("usage: zdlic machine | keygen <number> <key file> | issue --key <key file> ... --out <file> | show <file>".into()),
    }
}

fn keygen(args: &[String]) -> Result<(), String> {
    let [number, path] = args else { return Err("usage: zdlic keygen <number> <key file>".into()) };
    let number: u32 = number.parse().map_err(|_| format!("'{number}' isn't a key number"))?;
    let path = PathBuf::from(path);
    if path.exists() {
        return Err(format!("{} already exists; a key is never written over", path.display()));
    }
    let (file, public) = new_key(number)?;
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("couldn't make {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    let bytes: Vec<String> = public.iter().map(|b| format!("{b}")).collect();
    println!("Key {number} written to {}. Keep it off the repo and off any build machine.", path.display());
    println!("Its public key, for TRUSTED in crates/zd-licence/src/lib.rs:");
    println!("    ({number}, [{}]),", bytes.join(", "));
    Ok(())
}

fn issue(args: &[String]) -> Result<(), String> {
    let mut key = None;
    let mut number = None;
    let mut to = None;
    let mut modules = None;
    let mut machines = Vec::new();
    let mut until = None;
    let mut updates_until = None;
    let mut out = None;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut value = || rest.next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--key" => key = Some(value()?),
            "--licence" => number = Some(value()?),
            "--to" => to = Some(value()?),
            "--modules" => modules = Some(value()?),
            "--machine" => machines.push(value()?.to_uppercase()),
            "--until" => until = Some(date(&value()?)?),
            "--updates-until" => updates_until = Some(date(&value()?)?),
            "--out" => out = Some(PathBuf::from(value()?)),
            other => return Err(format!("no option '{other}'")),
        }
    }
    let need = |v: Option<String>, flag: &str| v.ok_or_else(|| format!("{flag} is needed"));
    let key_text = std::fs::read_to_string(need(key, "--key")?).map_err(|e| format!("couldn't read the key file: {e}"))?;
    let key_file: KeyFile = serde_json::from_str(&key_text).map_err(|e| format!("not a key file: {e}"))?;
    let modules = need(modules, "--modules")?;
    let modules: Vec<String> = if modules == "all" {
        MODULES.iter().map(|m| m.to_string()).collect()
    } else {
        modules.split(',').map(|m| m.trim().to_lowercase()).filter(|m| !m.is_empty()).collect()
    };
    if let Some(bad) = modules.iter().find(|m| !MODULES.contains(&m.as_str())) {
        return Err(format!("no module '{bad}'; the modules are {}", MODULES.join(", ")));
    }
    let licence = Licence {
        format: FORMAT,
        licence: need(number, "--licence")?,
        licensed_to: need(to, "--to")?,
        kind: Kind::Machine,
        modules,
        machines,
        seats: 1,
        issued: Day::today().to_string(),
        runs_until: until,
        updates_until,
    };
    let out = out.ok_or("--out is needed")?;
    std::fs::write(&out, sign(&licence, key_file.key, &key_file.signing_key()?)).map_err(|e| format!("couldn't write {}: {e}", out.display()))?;
    println!("Licence {} for {} written to {}", licence.licence, licence.licensed_to, out.display());
    Ok(())
}

fn date(text: &str) -> Result<String, String> {
    Day::parse(text).map(|d| d.to_string()).ok_or_else(|| format!("'{text}' isn't a date (YYYY-MM-DD)"))
}

fn show(args: &[String]) -> Result<(), String> {
    let [path] = args else { return Err("usage: zdlic show <file.zdlic>".into()) };
    let text = std::fs::read_to_string(path).map_err(|e| format!("couldn't read {path}: {e}"))?;
    let licence = read(&text, Keys::built_in())?;
    println!("{}", serde_json::to_string_pretty(&licence).map_err(|e| e.to_string())?);
    println!("Signed with a key this build trusts.");
    Ok(())
}

//! A machine's code: a hash of the id Windows gives the machine, never the id itself (decision 0098).

use sha2::{Digest, Sha256};

/// This machine's code, as `M-9C41-E2B7-55D0`. A machine whose id can't be read gets the code of an empty id, which no
/// licence lists.
pub fn machine_code() -> String {
    code_of(&machine_id().unwrap_or_default())
}

/// The code for a machine id, the same however the id's letters are cased.
pub fn code_of(id: &str) -> String {
    let hash = Sha256::digest(format!("zovlyn machine\n{}", id.trim().to_lowercase()).as_bytes());
    let hex: String = hash[..6].iter().map(|b| format!("{b:02X}")).collect();
    format!("M-{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
}

/// Windows' `MachineGuid`, read with `reg.exe` so nothing more is built in; no window shows while it runs.
#[cfg(windows)]
fn machine_id() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let reg = std::env::var_os("SystemRoot").map_or_else(|| "reg.exe".into(), |root| std::path::Path::new(&root).join("System32").join("reg.exe"));
    let out = std::process::Command::new(reg)
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid", "/reg:64"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with("MachineGuid"))?;
    line.split_whitespace().last().map(str::to_string)
}

#[cfg(not(windows))]
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"].iter().find_map(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_code_is_a_short_hash_the_same_whatever_the_case() {
        let code = code_of("3f2504e0-4f89-11d3-9a0c-0305e82c3301");
        assert_eq!(code.len(), 16);
        assert!(code.starts_with("M-") && code.matches('-').count() == 3, "{code}");
        assert_eq!(code, code_of(" 3F2504E0-4F89-11D3-9A0C-0305E82C3301 "));
        assert_ne!(code, code_of("3f2504e0-4f89-11d3-9a0c-0305e82c3302"));
        assert!(!code.contains("3f2504e0"), "the id itself is never in the code");
        assert_eq!(machine_code(), machine_code(), "this machine's code is the same each time");
    }
}

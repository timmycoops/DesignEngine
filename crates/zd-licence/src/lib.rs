//! Licence files for the Zovlyn Pit Design Engine (decision 0098).
//!
//! A licence is a few lines of JSON signed with an Ed25519 key only Zovlyn holds. The app carries the matching public
//! keys ([`TRUSTED`]) and checks a licence on the machine, with no internet: one changed character and the signature
//! fails. A licence turns on modules by name; with none, or once one lapses, the app is the Viewer, and a project always
//! opens. The engine never reads a licence: the app and the Python bindings read it here and tell the engine which
//! modules are on.

mod day;
mod machine;

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub use day::Day;
pub use machine::{code_of, machine_code};

/// The licence format this build writes and reads.
pub const FORMAT: u32 = 1;
/// Days the app runs on after a licence's end, with a warning.
pub const GRACE_DAYS: i64 = 14;
/// Days before a licence's end that the app starts to say so.
pub const WARN_DAYS: i64 = 30;
/// A clock may read this many days before the newest day the app has seen (time zones, a day's slack).
pub const CLOCK_SLACK_DAYS: i64 = 1;

/// Module names a licence may list (the engine's `Module` names; a test in the app holds them the same).
pub const MODULES: [&str; 5] = ["viewer", "design", "solids", "reserves", "python"];

/// Public keys this build trusts, by number. The signing keys are kept by Zovlyn, off any build machine.
pub const TRUSTED: &[(u32, [u8; 32])] = &[
    // Key 1, made 2026-11-22.
    (1, [127, 158, 189, 50, 138, 14, 99, 143, 162, 162, 82, 76, 190, 164, 241, 189, 108, 79, 98, 106, 230, 160, 23, 216, 140, 72, 212, 199, 96, 144, 116, 98]),
];
/// Keys no longer trusted, by number, though once listed above: a key that leaked.
pub const RETIRED: &[u32] = &[];

/// What a licence says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Licence {
    pub format: u32,
    /// The licence number, such as `ZD-0001`.
    pub licence: String,
    pub licensed_to: String,
    pub kind: Kind,
    /// Module names, from [`MODULES`].
    pub modules: Vec<String>,
    /// Machine codes it runs on; none means any machine.
    #[serde(default)]
    pub machines: Vec<String>,
    /// Seats on each machine; one for now, counted later (decision 0098).
    #[serde(default = "one")]
    pub seats: u32,
    /// `YYYY-MM-DD`.
    pub issued: String,
    /// The last day it runs, before its grace; none runs on.
    #[serde(default)]
    pub runs_until: Option<String>,
    /// The last day an update may be installed under it; none, every update.
    #[serde(default)]
    pub updates_until: Option<String>,
}

fn one() -> u32 {
    1
}

/// How a licence is counted. Floating seats on a site server come later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Machine,
}

/// A licence file as written: the licence's JSON exactly as it was signed, the key's number and the signature.
#[derive(Serialize, Deserialize)]
struct SignedFile {
    licence: String,
    key: u32,
    signature: String,
}

const SIGNED_PREFIX: &[u8] = b"zovlyn licence\n";

fn signed_bytes(licence_json: &str) -> Vec<u8> {
    [SIGNED_PREFIX, licence_json.as_bytes()].concat()
}

/// The public keys a check trusts and the numbers it no longer does.
#[derive(Clone, Copy)]
pub struct Keys<'a> {
    pub trusted: &'a [(u32, [u8; 32])],
    pub retired: &'a [u32],
}

impl Keys<'static> {
    /// The keys built into this version.
    pub fn built_in() -> Keys<'static> {
        Keys { trusted: TRUSTED, retired: RETIRED }
    }
}

/// Signs a licence: the text of a `.zdlic` file.
pub fn sign(licence: &Licence, key_number: u32, key: &SigningKey) -> String {
    let json = serde_json::to_string(licence).expect("a licence always writes");
    let signature = key.sign(&signed_bytes(&json));
    let file = SignedFile { licence: json, key: key_number, signature: BASE64.encode(signature.to_bytes()) };
    serde_json::to_string_pretty(&file).expect("a licence file always writes") + "\n"
}

/// Reads a licence file and checks its signature; the reason it's refused if not.
pub fn read(text: &str, keys: Keys<'_>) -> Result<Licence, String> {
    let file: SignedFile = serde_json::from_str(text).map_err(|e| format!("The file isn't a licence this version can read ({e})"))?;
    if keys.retired.contains(&file.key) {
        return Err(format!("The licence was signed with key {}, which is no longer trusted", file.key));
    }
    let public = keys.trusted.iter().find(|(n, _)| *n == file.key).map(|(_, k)| k).ok_or_else(|| format!("The licence was signed with key {}, which this version doesn't know", file.key))?;
    let public = VerifyingKey::from_bytes(public).map_err(|_| format!("Key {} built into this version isn't a valid key", file.key))?;
    let signature = BASE64.decode(file.signature.trim()).ok().and_then(|b| Signature::from_slice(&b).ok());
    let signed = signature.is_some_and(|s| public.verify(&signed_bytes(&file.licence), &s).is_ok());
    if !signed {
        return Err("The licence's signature doesn't match: the file has been changed, or wasn't issued by Zovlyn".into());
    }
    let licence: Licence = serde_json::from_str(&file.licence).map_err(|e| format!("The licence was made for a newer version of the app ({e})"))?;
    if licence.format > FORMAT {
        return Err(format!("The licence was made for a newer version of the app (format {})", licence.format));
    }
    Ok(licence)
}

/// Where a licence stands on this machine today.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    /// Running; `days_left` to its end, if it has one.
    Current { days_left: Option<i64> },
    /// Running, and ending within [`WARN_DAYS`].
    EndsSoon { days_left: i64 },
    /// Past its end, within [`GRACE_DAYS`].
    Grace { grace_left: i64 },
    /// The Viewer only, and why.
    Viewer { reason: String },
}

/// What the app shows of its licence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub licence: Option<Licence>,
    #[serde(flatten)]
    pub state: State,
    /// The modules on: the licence's, or the Viewer alone.
    pub modules: Vec<String>,
    /// This machine's code, to send to Zovlyn for a licence.
    pub machine: String,
    /// Where the licence was read from, if one was found.
    pub path: Option<String>,
}

impl Status {
    /// The line written on reports and in project manifests while a licence is running: none in the Viewer.
    pub fn stamp(&self) -> Option<String> {
        match (&self.state, &self.licence) {
            (State::Viewer { .. }, _) | (_, None) => None,
            (_, Some(l)) => Some(format!("{}, licensed to {}", l.licence, l.licensed_to)),
        }
    }

    /// Whether a module, by name, is on.
    pub fn has(&self, module: &str) -> bool {
        self.modules.iter().any(|m| m == module)
    }
}

/// Checks a licence file's text (none if there's no file) for this machine, today, given the newest day the app has
/// seen on it.
pub fn check(text: Option<&str>, keys: Keys<'_>, machine: &str, today: Day, last_seen: Option<Day>) -> Status {
    let viewer = |licence: Option<Licence>, reason: String| Status {
        licence,
        state: State::Viewer { reason },
        modules: vec!["viewer".into()],
        machine: machine.to_string(),
        path: None,
    };
    let Some(text) = text else {
        return viewer(None, "No licence on this machine: the app is the Viewer".into());
    };
    let licence = match read(text, keys) {
        Ok(licence) => licence,
        Err(reason) => return viewer(None, reason),
    };
    if !licence.machines.is_empty() && !licence.machines.iter().any(|m| m.eq_ignore_ascii_case(machine)) {
        return viewer(Some(licence), format!("The licence is for other machines; this machine's code is {machine}"));
    }
    if let Some(seen) = last_seen.filter(|seen| today < seen.plus(-CLOCK_SLACK_DAYS)) {
        return viewer(Some(licence), format!("The computer's clock reads {today}, before {seen}, the last day the app ran here: set the clock right"));
    }
    let end = match licence.runs_until.as_deref().map(|d| Day::parse(d).ok_or(d)) {
        None => None,
        Some(Ok(end)) => Some(end),
        Some(Err(d)) => {
            let reason = format!("The licence's end date '{d}' isn't a date");
            return viewer(Some(licence), reason);
        }
    };
    let state = match end.map(|end| end.0 - today.0) {
        None => State::Current { days_left: None },
        Some(left) if left > WARN_DAYS => State::Current { days_left: Some(left) },
        Some(left) if left >= 0 => State::EndsSoon { days_left: left },
        Some(left) if -left <= GRACE_DAYS => State::Grace { grace_left: GRACE_DAYS + left },
        Some(_) => {
            let end = end.expect("a number of days left means an end");
            return viewer(Some(licence), format!("The licence ran until {end}, and its {GRACE_DAYS} days' grace are over"));
        }
    };
    let mut modules: Vec<String> = licence.modules.clone();
    if !modules.iter().any(|m| m == "viewer") {
        modules.insert(0, "viewer".into());
    }
    Status { licence: Some(licence), state, modules, machine: machine.to_string(), path: None }
}

/// The folders a licence is looked for in, the user's first: `%APPDATA%\Zovlyn`, then `%PROGRAMDATA%\Zovlyn` (one an
/// administrator puts there for every user of the machine).
pub fn folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if cfg!(windows) {
        folders.extend(std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Zovlyn")));
        folders.extend(std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join("Zovlyn")));
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        folders.extend(config.map(|c| c.join("zovlyn")));
    }
    folders
}

/// The licence file's name in those folders.
pub const FILE_NAME: &str = "licence.zdlic";
const SEEN_NAME: &str = "seen";

/// The licence file in use: the one `ZD_LICENCE` names, else the first in [`folders`].
pub fn find() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ZD_LICENCE").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(path);
    }
    folders().into_iter().map(|f| f.join(FILE_NAME)).find(|p| p.is_file())
}

/// Checks this machine's licence today, and keeps today as the newest day seen (unless the clock is behind it).
pub fn check_this_machine() -> Status {
    let path = find();
    let text = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    let seen_file = folders().into_iter().next().map(|f| f.join(SEEN_NAME));
    let last_seen = seen_file.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| Day::parse(&t));
    let today = Day::today();
    let mut status = check(text.as_deref(), Keys::built_in(), &machine_code(), today, last_seen);
    status.path = path.map(|p| p.display().to_string());
    if last_seen.is_none_or(|seen| today > seen)
        && let Some(seen_file) = seen_file
    {
        let _ = seen_file.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(seen_file, today.to_string());
    }
    status
}

/// Puts a licence file in the user's folder, once it's a licence this version trusts, and checks it.
pub fn install(from: &Path) -> Result<Status, String> {
    let text = std::fs::read_to_string(from).map_err(|e| format!("Couldn't read {}: {e}", from.display()))?;
    read(&text, Keys::built_in())?;
    let folder = folders().into_iter().next().ok_or("No folder for the licence on this machine")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    std::fs::write(folder.join(FILE_NAME), text).map_err(|e| format!("Couldn't write the licence: {e}"))?;
    Ok(check_this_machine())
}

/// A signing key as kept in a `.zdkey` file: its number and its 32 secret bytes.
#[derive(Serialize, Deserialize)]
pub struct KeyFile {
    pub key: u32,
    pub secret: String,
}

impl KeyFile {
    pub fn signing_key(&self) -> Result<SigningKey, String> {
        let bytes = BASE64.decode(self.secret.trim()).map_err(|e| format!("not a key file: {e}"))?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| "not a key file: the secret isn't 32 bytes".to_string())?;
        Ok(SigningKey::from_bytes(&bytes))
    }
}

/// A new signing key, from the system's randomness.
pub fn new_key(number: u32) -> Result<(KeyFile, [u8; 32]), String> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("no randomness: {e}"))?;
    let public = SigningKey::from_bytes(&secret).verifying_key().to_bytes();
    Ok((KeyFile { key: number, secret: BASE64.encode(secret) }, public))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_SECRET: [u8; 32] = [7; 32];
    const MACHINE: &str = "M-1111-2222-3333";

    fn key() -> SigningKey {
        SigningKey::from_bytes(&KEY_SECRET)
    }

    fn trusted() -> [(u32, [u8; 32]); 1] {
        [(9, key().verifying_key().to_bytes())]
    }

    fn licence(until: Option<&str>) -> Licence {
        Licence {
            format: FORMAT,
            licence: "ZD-0001".into(),
            licensed_to: "Example Coal Pty Ltd".into(),
            kind: Kind::Machine,
            modules: vec!["design".into(), "solids".into()],
            machines: vec![MACHINE.into()],
            seats: 1,
            issued: "2026-11-22".into(),
            runs_until: until.map(str::to_string),
            updates_until: None,
        }
    }

    fn day(text: &str) -> Day {
        Day::parse(text).unwrap()
    }

    fn check_on(text: &str, today: &str, last_seen: Option<&str>) -> Status {
        let trusted = trusted();
        check(Some(text), Keys { trusted: &trusted, retired: &[] }, MACHINE, day(today), last_seen.map(day))
    }

    fn reason(status: &Status) -> &str {
        match &status.state {
            State::Viewer { reason } => reason,
            other => panic!("expected the Viewer, got {other:?}"),
        }
    }

    #[test]
    fn a_licence_signed_reads_back_the_same() {
        let text = sign(&licence(Some("2027-06-30")), 9, &key());
        let trusted = trusted();
        assert_eq!(read(&text, Keys { trusted: &trusted, retired: &[] }), Ok(licence(Some("2027-06-30"))));
        let status = check_on(&text, "2026-11-22", None);
        assert_eq!(status.state, State::Current { days_left: Some(220) });
        assert_eq!(status.modules, ["viewer", "design", "solids"]);
        assert_eq!(status.stamp().as_deref(), Some("ZD-0001, licensed to Example Coal Pty Ltd"));
        assert!(status.has("solids") && !status.has("python"));
    }

    #[test]
    fn one_character_changed_fails_the_signature() {
        let text = sign(&licence(None), 9, &key());
        let changed = text.replacen("solids", "reserves", 1);
        assert_ne!(changed, text);
        let status = check_on(&changed, "2026-11-22", None);
        assert!(reason(&status).contains("signature doesn't match"), "{status:?}");
        assert_eq!(status.modules, ["viewer"]);
        // Signed by another key under the same number.
        let forged = sign(&licence(None), 9, &SigningKey::from_bytes(&[8; 32]));
        assert!(reason(&check_on(&forged, "2026-11-22", None)).contains("signature doesn't match"));
    }

    #[test]
    fn a_retired_or_unknown_key_is_refused() {
        let text = sign(&licence(None), 9, &key());
        let trusted = trusted();
        let retired = check(Some(&text), Keys { trusted: &trusted, retired: &[9] }, MACHINE, day("2026-11-22"), None);
        assert!(reason(&retired).contains("no longer trusted"), "{retired:?}");
        let unknown = sign(&licence(None), 4, &key());
        assert!(reason(&check_on(&unknown, "2026-11-22", None)).contains("key 4"));
    }

    #[test]
    fn a_licence_for_another_machine_is_the_viewer_and_none_listed_runs_anywhere() {
        let mut other = licence(None);
        other.machines = vec!["M-9999-9999-9999".into()];
        let status = check_on(&sign(&other, 9, &key()), "2026-11-22", None);
        assert!(reason(&status).contains(MACHINE), "{status:?}");
        other.machines.clear();
        assert_eq!(check_on(&sign(&other, 9, &key()), "2026-11-22", None).state, State::Current { days_left: None });
    }

    #[test]
    fn the_end_warns_then_grace_then_the_viewer() {
        let text = sign(&licence(Some("2027-06-30")), 9, &key());
        assert_eq!(check_on(&text, "2027-06-20", None).state, State::EndsSoon { days_left: 10 });
        assert_eq!(check_on(&text, "2027-06-30", None).state, State::EndsSoon { days_left: 0 });
        assert_eq!(check_on(&text, "2027-07-01", None).state, State::Grace { grace_left: 13 });
        assert_eq!(check_on(&text, "2027-07-14", None).state, State::Grace { grace_left: 0 });
        let over = check_on(&text, "2027-07-15", None);
        assert!(reason(&over).contains("grace are over"), "{over:?}");
    }

    #[test]
    fn a_clock_wound_back_is_caught() {
        let text = sign(&licence(None), 9, &key());
        assert!(reason(&check_on(&text, "2026-11-15", Some("2026-11-22"))).contains("clock"));
        // A day's slack for time zones.
        assert_eq!(check_on(&text, "2026-11-21", Some("2026-11-22")).state, State::Current { days_left: None });
    }

    #[test]
    fn no_file_or_a_file_that_isnt_a_licence_is_the_viewer() {
        let trusted = trusted();
        let keys = Keys { trusted: &trusted, retired: &[] };
        let none = check(None, keys, MACHINE, day("2026-11-22"), None);
        assert!(reason(&none).starts_with("No licence"));
        assert_eq!(none.modules, ["viewer"]);
        assert_eq!(none.stamp(), None);
        assert!(reason(&check(Some("hello"), keys, MACHINE, day("2026-11-22"), None)).contains("isn't a licence"));
    }

    #[test]
    fn a_key_file_signs_as_its_key() {
        let (file, public) = new_key(3).unwrap();
        let text = sign(&licence(None), file.key, &file.signing_key().unwrap());
        let status = check(Some(&text), Keys { trusted: &[(3, public)], retired: &[] }, MACHINE, day("2026-11-22"), None);
        assert_eq!(status.state, State::Current { days_left: None });
    }
}

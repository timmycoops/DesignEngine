//! 128-bit ids for objects, layers and command runs, generated in-house (decision 0004).
//!
//! Layout: 48-bit Unix milliseconds, 16-bit session salt, 64-bit counter. Ids made in one session
//! sort in creation order (unless the system clock steps backwards); the salt keeps two sessions that
//! start in the same millisecond apart. They print and serialise as 32 hex digits.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SALT: OnceLock<u16> = OnceLock::new();

fn next_raw() -> u128 {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis()) & ((1 << 48) - 1);
    let salt = *SALT.get_or_init(session_salt);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    (ms << 80) | (u128::from(salt) << 64) | u128::from(count)
}

/// Sixteen bits that differ between sessions: the process id and start time, mixed by the standard
/// library's randomly keyed hasher.
fn session_salt() -> u16 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    hasher.finish() as u16
}

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u128);

        impl $name {
            /// A fresh id, later than any made before it in this session.
            pub fn new() -> Self {
                $name(next_raw())
            }

            pub fn from_u128(raw: u128) -> Self {
                $name(raw)
            }

            pub fn as_u128(self) -> u128 {
                self.0
            }

            /// Parses the 32-hex-digit form.
            pub fn parse(hex: &str) -> Option<Self> {
                if hex.len() != 32 {
                    return None;
                }
                u128::from_str_radix(hex, 16).ok().map($name)
            }
        }

        /// Same as `new`: a fresh id.
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:032x}", self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({:032x})"), self.0)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(deserializer)?;
                $name::parse(&text)
                    .ok_or_else(|| serde::de::Error::custom(format!("expected 32 hex digits, found '{text}'")))
            }
        }
    };
}

define_id!(
    /// An object (string, surface, solid or grid). Every revision of the object shares it.
    ObjectId
);
define_id!(
    /// A layer.
    LayerId
);
define_id!(
    /// One run of a command: the provenance of every revision it made.
    RunId
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_sort_in_creation_order() {
        let ids: Vec<ObjectId> = (0..100_000).map(|_| ObjectId::new()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "creation order");
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    }

    #[test]
    fn hex_text_round_trips() {
        let id = RunId::new();
        let text = id.to_string();
        assert_eq!(text.len(), 32);
        assert_eq!(RunId::parse(&text), Some(id));
        assert_eq!(RunId::parse("abc"), None);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<RunId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<RunId>("\"not-an-id\"").is_err());
    }
}

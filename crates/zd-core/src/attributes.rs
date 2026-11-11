//! An object's attributes, with each name held once for the whole program (decision 0058, stage 3).
//!
//! A block solid carries hundreds of attributes, and a pit hundreds of thousands of solids, nearly all with the same
//! names. A map of its own strings on every object held each name hundreds of thousands of times: 11 million strings
//! on a 43,000-solid project. Here a name is interned once and every object holds a pointer to it, beside its value,
//! in a short list kept in name order. It reads and iterates as the map did, in the same order, and saves to the same
//! JSON.
//!
//! An interned name is never freed. Attribute names are column names (a few hundred to a project, a few thousand at
//! most), so that is bounded in practice.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::sync::{LazyLock, Mutex};

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::AttrValue;

static NAMES: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

thread_local! {
    /// Names this thread has seen, so it takes the shared lock only for a name new to it.
    static SEEN: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
}

/// The one copy of `name`, shared by every object that has an attribute of that name.
pub fn intern(name: &str) -> &'static str {
    if let Some(found) = SEEN.with(|seen| seen.borrow().get(name).copied()) {
        return found;
    }
    let shared = {
        let mut names = NAMES.lock().unwrap_or_else(|p| p.into_inner());
        match names.get(name) {
            Some(&found) => found,
            None => {
                let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
                names.insert(leaked);
                leaked
            }
        }
    };
    SEEN.with(|seen| seen.borrow_mut().insert(shared));
    shared
}

/// An object's attributes: values by name, in name order, each name held once for the program.
#[derive(Clone, Default, PartialEq)]
pub struct Attributes {
    /// Sorted by name, each name once.
    entries: Vec<(&'static str, AttrValue)>,
}

impl Attributes {
    pub fn new() -> Self {
        Attributes::default()
    }

    fn find(&self, name: &str) -> Result<usize, usize> {
        self.entries.binary_search_by(|(n, _)| (*n).cmp(name))
    }

    pub fn get(&self, name: &str) -> Option<&AttrValue> {
        self.find(name).ok().map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut AttrValue> {
        self.find(name).ok().map(|i| &mut self.entries[i].1)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.find(name).is_ok()
    }

    /// Sets a value, returning the one it replaces. The name is a `String`, as the map's was, so `"ash".into()` reads
    /// as it did.
    pub fn insert(&mut self, name: String, value: AttrValue) -> Option<AttrValue> {
        match self.find(&name) {
            Ok(i) => Some(std::mem::replace(&mut self.entries[i].1, value)),
            Err(i) => {
                self.entries.insert(i, (intern(&name), value));
                None
            }
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<AttrValue> {
        self.find(name).ok().map(|i| self.entries.remove(i).1)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Names and values in name order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&'static str, &AttrValue)> + DoubleEndedIterator {
        self.entries.iter().map(|(n, v)| (*n, v))
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &'static str> + DoubleEndedIterator + '_ {
        self.entries.iter().map(|(n, _)| *n)
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &AttrValue> + DoubleEndedIterator {
        self.entries.iter().map(|(_, v)| v)
    }

    /// Keeps only the attributes `keep` says to.
    pub fn retain(&mut self, mut keep: impl FnMut(&str, &mut AttrValue) -> bool) {
        self.entries.retain_mut(|(n, v)| keep(n, v));
    }

    /// Attributes from names already interned, in any order; a name given twice keeps its last value.
    fn from_entries(mut entries: Vec<(&'static str, AttrValue)>) -> Self {
        // A stable sort keeps the given order among equal names, so the last of them is the one kept.
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut out: Vec<(&'static str, AttrValue)> = Vec::with_capacity(entries.len());
        for entry in entries {
            match out.last_mut() {
                Some(last) if last.0 == entry.0 => *last = entry,
                _ => out.push(entry),
            }
        }
        out.shrink_to_fit();
        Attributes { entries: out }
    }
}

impl std::ops::Index<&str> for Attributes {
    type Output = AttrValue;

    fn index(&self, name: &str) -> &AttrValue {
        self.get(name).unwrap_or_else(|| panic!("no attribute '{name}'"))
    }
}

impl<K: AsRef<str>> FromIterator<(K, AttrValue)> for Attributes {
    fn from_iter<I: IntoIterator<Item = (K, AttrValue)>>(iter: I) -> Self {
        Attributes::from_entries(iter.into_iter().map(|(k, v)| (intern(k.as_ref()), v)).collect())
    }
}

impl<K: AsRef<str>> Extend<(K, AttrValue)> for Attributes {
    fn extend<I: IntoIterator<Item = (K, AttrValue)>>(&mut self, iter: I) {
        let mut entries = std::mem::take(&mut self.entries);
        entries.extend(iter.into_iter().map(|(k, v)| (intern(k.as_ref()), v)));
        *self = Attributes::from_entries(entries);
    }
}

impl<K: AsRef<str>, const N: usize> From<[(K, AttrValue); N]> for Attributes {
    fn from(entries: [(K, AttrValue); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a Attributes {
    type Item = (&'static str, &'a AttrValue);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (&'static str, AttrValue)>, fn(&'a (&'static str, AttrValue)) -> (&'static str, &'a AttrValue)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(n, v)| (*n, v))
    }
}

impl IntoIterator for Attributes {
    type Item = (&'static str, AttrValue);
    type IntoIter = std::vec::IntoIter<(&'static str, AttrValue)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl fmt::Debug for Attributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl Serialize for Attributes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (name, value) in &self.entries {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Attributes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Names;
        impl<'de> Visitor<'de> for Names {
            type Value = Attributes;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of attribute names to values")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Attributes, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(name) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                    entries.push((intern(&name), map.next_value::<AttrValue>()?));
                }
                Ok(Attributes::from_entries(entries))
            }
        }
        deserializer.deserialize_map(Names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_held_once_for_every_object() {
        let mut a = Attributes::new();
        let mut b = Attributes::new();
        a.insert("seam".into(), AttrValue::Text("A".into()));
        b.insert(String::from("seam"), AttrValue::Text("B".into()));
        let (x, y) = (a.keys().next().unwrap(), b.keys().next().unwrap());
        assert!(std::ptr::eq(x, y), "one copy of the name");
    }

    #[test]
    fn it_reads_iterates_and_saves_as_the_map_did() {
        let mut a = Attributes::new();
        for (name, value) in [("strip", 12.0), ("bench", 3.0), ("ash", 11.5)] {
            assert_eq!(a.insert(name.into(), AttrValue::Number(value)), None);
        }
        assert_eq!(a.insert("bench".into(), AttrValue::Number(4.0)), Some(AttrValue::Number(3.0)), "replaces, as a map does");
        assert_eq!(a.keys().collect::<Vec<_>>(), ["ash", "bench", "strip"], "name order, as the map's");
        assert_eq!(a["bench"], AttrValue::Number(4.0));
        assert!(a.contains_key("ash") && !a.contains_key("rd"));
        assert_eq!(a.remove("ash"), Some(AttrValue::Number(11.5)));
        assert_eq!(a.len(), 2);

        let map: std::collections::BTreeMap<String, AttrValue> = [("b".to_string(), AttrValue::Bool(true)), ("a".to_string(), AttrValue::Text("x".into()))].into();
        let ours: Attributes = map.clone().into_iter().collect();
        assert_eq!(serde_json::to_string(&ours).unwrap(), serde_json::to_string(&map).unwrap(), "the same JSON");
        let back: Attributes = serde_json::from_str(r#"{"z": 1.5, "a": "x", "z": 2.5}"#).unwrap();
        assert_eq!(back.iter().collect::<Vec<_>>(), [("a", &AttrValue::Text("x".into())), ("z", &AttrValue::Number(2.5))], "sorted, the last of a name kept");
    }
}

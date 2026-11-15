//! Filters on what objects carry, one way everywhere (decisions 0071 and 0072): the bench rules pick solids with them and
//! parameter tables pick rows with them. A filter names attributes, each with a comma list of the values it may hold:
//! words, numbers or ranges such as `1-5` (`*` or blank is any). Names and words match with their case aside.

use std::collections::BTreeMap;

use crate::{AttrValue, Attributes};

/// An attribute's value by name, the name's case aside.
pub fn attribute_ci<'a>(attributes: &'a Attributes, key: &str) -> Option<&'a AttrValue> {
    attributes.get(key).or_else(|| attributes.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
}

/// A value as a word: a whole number without its decimal point, text trimmed.
pub fn attribute_word(value: &AttrValue) -> String {
    match value {
        AttrValue::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
        AttrValue::Number(n) => n.to_string(),
        AttrValue::Text(t) => t.trim().to_string(),
        AttrValue::Bool(b) => b.to_string(),
    }
}

/// A range such as `1-5` or `-20--5`: two numbers either side of a dash, lowest first. `U-M` is no range.
fn range(item: &str) -> Option<(f64, f64)> {
    item.char_indices().skip(1).filter(|&(_, c)| c == '-').find_map(|(i, _)| {
        let (a, b) = (item[..i].trim().parse::<f64>().ok()?, item[i + 1..].trim().parse::<f64>().ok()?);
        Some((a.min(b), a.max(b)))
    })
}

/// Whether one item of a filter's list fits a value: `*`, a range or a number for a value that's a number (or text that
/// reads as one), or the same word, case aside.
fn item_fits(item: &str, value: &AttrValue) -> bool {
    if item == "*" {
        return true;
    }
    let number = match value {
        AttrValue::Number(n) => Some(*n),
        AttrValue::Text(t) => t.trim().parse::<f64>().ok(),
        AttrValue::Bool(_) => None,
    };
    if let Some(n) = number {
        if let Some((a, b)) = range(item) {
            return a <= n && n <= b;
        }
        if let Ok(x) = item.parse::<f64>() {
            return (x - n).abs() <= 1e-9 * x.abs().max(1.0);
        }
    }
    attribute_word(value).eq_ignore_ascii_case(item)
}

/// Whether a filter's comma list fits a value; a blank list fits anything, and a list that isn't blank never fits a value
/// that isn't there.
pub fn list_fits(list: &str, value: Option<&AttrValue>) -> bool {
    let items: Vec<&str> = list.split(',').map(str::trim).filter(|i| !i.is_empty()).collect();
    if items.is_empty() || items.contains(&"*") {
        return true;
    }
    value.is_some_and(|v| items.iter().any(|item| item_fits(item, v)))
}

/// Whether attributes fit a filter: every attribute it names fits its list.
pub fn filter_fits(when: &BTreeMap<String, String>, attributes: &Attributes) -> bool {
    when.iter().all(|(key, list)| list_fits(list, attribute_ci(attributes, key)))
}

/// A filter in a few words, `everything` when it names nothing: `horizon U-M, M-L, strip 1-5`.
pub fn filter_words(when: &BTreeMap<String, String>) -> String {
    let parts: Vec<String> = when.iter().filter(|(_, l)| !l.trim().is_empty()).map(|(k, l)| format!("{k} {}", l.trim())).collect();
    if parts.is_empty() { "everything".to_string() } else { parts.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, AttrValue)]) -> Attributes {
        let mut a = Attributes::new();
        for (k, v) in pairs {
            a.insert(k.to_string(), v.clone());
        }
        a
    }

    #[test]
    fn lists_ranges_words_and_case() {
        let a = attrs(&[("Strip", AttrValue::Number(3.0)), ("horizon", AttrValue::Text("U-M".into()))]);
        assert_eq!(range("-20--5"), Some((-20.0, -5.0)));
        assert_eq!(range("U-M"), None);
        assert!(list_fits("1-5", attribute_ci(&a, "strip")));
        assert!(!list_fits("4-5, 7", attribute_ci(&a, "strip")));
        assert!(list_fits("u-m, M-L", attribute_ci(&a, "horizon")), "U-M is a word, not a range");
        assert!(list_fits("", None) && list_fits("*", None));
        assert!(!list_fits("1", None));
        let when: BTreeMap<String, String> = [("strip".to_string(), "1-5".to_string()), ("horizon".to_string(), "U-M".to_string())].into();
        assert!(filter_fits(&when, &a));
        assert_eq!(filter_words(&when), "horizon U-M, strip 1-5");
        assert_eq!(filter_words(&BTreeMap::new()), "everything");
    }
}

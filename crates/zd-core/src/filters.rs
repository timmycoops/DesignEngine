//! Filters on what objects carry, one way everywhere (decisions 0071 and 0072, and the Filtering plan's decisions 1, 2
//! and 8): the bench rules pick solids with them, parameter tables pick rows with them, and a workflow step's input picks
//! objects with them. A filter names attributes, each with a comma list of the values it may hold: words, numbers, ranges
//! such as `1-5`, or comparisons such as `<0.3` and `>=120` (`*` or blank is any). An item starting `!` is a value it may
//! not hold (`!WASTE`). Names and words match with their case aside. On an object, a name it carries no attribute of may
//! be one of its measures: `volume`, `area`, `length`, `rl_min` and `rl_max`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::{AttrValue, Attributes, Body, Object};

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

/// A comparison such as `<0.3`, `<= 5`, `>120` or `>=-20`: how it compares and the number it compares with. `<abc` is
/// none, and is read as a word.
fn comparison(item: &str) -> Option<(&'static str, f64)> {
    let (op, rest) = ["<=", ">=", "<", ">"].into_iter().find_map(|op| item.strip_prefix(op).map(|rest| (op, rest)))?;
    rest.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| (op, x))
}

/// Whether one item of a filter's list fits a value: `*`, a comparison, a range or a number for a value that's a number
/// (or text that reads as one), or the same word, case aside. A comparison never fits a word.
fn item_fits(item: &str, value: &AttrValue) -> bool {
    if item == "*" {
        return true;
    }
    let number = match value {
        AttrValue::Number(n) => Some(*n),
        AttrValue::Text(t) => t.trim().parse::<f64>().ok(),
        AttrValue::Bool(_) => None,
    };
    let compared = comparison(item);
    if let Some(n) = number {
        if let Some((op, x)) = compared {
            return match op {
                "<" => n < x,
                "<=" => n <= x,
                ">" => n > x,
                _ => n >= x,
            };
        }
        if let Some((a, b)) = range(item) {
            return a <= n && n <= b;
        }
        if let Ok(x) = item.parse::<f64>() {
            return (x - n).abs() <= 1e-9 * x.abs().max(1.0);
        }
    }
    compared.is_none() && attribute_word(value).eq_ignore_ascii_case(item)
}

/// Whether a filter's comma list fits a value. A blank list fits anything. Its items without `!` are the values it may
/// hold: one must fit, so a value that isn't there fits none. Its items with `!` are values it may not hold: none may
/// fit, so a value that isn't there fits `!WASTE`, as it isn't waste.
pub fn list_fits(list: &str, value: Option<&AttrValue>) -> bool {
    let items: Vec<&str> = list.split(',').map(str::trim).filter(|i| !i.is_empty()).collect();
    let (not, may): (Vec<&str>, Vec<&str>) = items.into_iter().partition(|i| i.starts_with('!'));
    let not: Vec<&str> = not.into_iter().map(|i| i[1..].trim()).filter(|i| !i.is_empty()).collect();
    let may_fits = may.is_empty() || may.contains(&"*") || value.is_some_and(|v| may.iter().any(|item| item_fits(item, v)));
    may_fits && !value.is_some_and(|v| not.iter().any(|item| item_fits(item, v)))
}

/// Whether attributes fit a filter: every attribute it names fits its list.
pub fn filter_fits(when: &BTreeMap<String, String>, attributes: &Attributes) -> bool {
    when.iter().all(|(key, list)| list_fits(list, attribute_ci(attributes, key)))
}

/// The measures a filter can name on an object, as it names an attribute (Filtering plan, decision 2).
pub const MEASURES: [&str; 5] = ["volume", "area", "length", "rl_min", "rl_max"];

/// One of an object's measures by name, case aside, where it has it: a solid's `volume` (m³, the volume of record); a
/// string's `length` (m, along it in 3D); a closed string's `area` (m², in plan) or a surface's (m², over its triangles);
/// and the lowest and highest RL of anything with heights (`rl_min`, `rl_max`), a grid's over the nodes that have one.
pub fn measure(object: &Object, key: &str) -> Option<f64> {
    let key = key.to_ascii_lowercase();
    match (key.as_str(), &object.body) {
        ("volume", Body::Solid(s)) => Some(s.volume()),
        ("length", Body::String(p)) => Some(p.length()),
        ("area", Body::String(p)) => p.plan_area(),
        ("area", Body::Surface(s)) => Some(s.mesh().area()),
        ("rl_min" | "rl_max", body) => {
            let heights: Box<dyn Iterator<Item = f64> + '_> = match body {
                Body::String(p) => Box::new(p.vertices.iter().map(|v| v[2])),
                Body::Surface(s) => Box::new(s.mesh().vertices.iter().map(|v| v[2])),
                Body::Solid(s) => Box::new(s.mesh().vertices.iter().map(|v| v[2])),
                Body::Grid(g) => Box::new(g.z.iter().copied()),
                _ => return None,
            };
            let finite = heights.filter(|z| z.is_finite());
            let lowest = key == "rl_min";
            finite.reduce(|a, b| if lowest { a.min(b) } else { a.max(b) })
        }
        _ => None,
    }
}

/// What a filter reads for a name on an object: its attribute of that name, case aside, or else its measure.
pub fn object_value<'a>(object: &'a Object, key: &str) -> Option<Cow<'a, AttrValue>> {
    attribute_ci(&object.attributes, key).map(Cow::Borrowed).or_else(|| measure(object, key).map(|m| Cow::Owned(AttrValue::Number(m))))
}

/// Whether an object fits a filter: every name it gives, an attribute or a measure, fits its list.
pub fn object_fits(when: &BTreeMap<String, String>, object: &Object) -> bool {
    when.iter().all(|(key, list)| list_fits(list, object_value(object, key).as_deref()))
}

/// A workflow step input's filter (decision 0065; Filtering plan, decision 8). A word is read as a filter's list, so
/// `1-5`, `<500` or `!WASTE` work there as anywhere; a number or a yes/no saved as one (as a capture writes them, and
/// every workflow before this did) is that value.
pub fn input_filter_fits(filter: &BTreeMap<String, AttrValue>, object: &Object) -> bool {
    filter.iter().all(|(key, value)| {
        let list: Cow<str> = match value {
            AttrValue::Text(t) => Cow::Borrowed(t),
            other => Cow::Owned(attribute_word(other)),
        };
        list_fits(&list, object_value(object, key).as_deref())
    })
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

    #[test]
    fn comparisons_take_numbers_and_never_words() {
        let a = attrs(&[("thickness", AttrValue::Number(0.25)), ("rl", AttrValue::Text("120".into())), ("seam", AttrValue::Text("U".into()))]);
        let fits = |list: &str, key: &str| list_fits(list, attribute_ci(&a, key));
        assert!(fits("<0.3", "thickness") && !fits("<0.25", "thickness") && fits("<=0.25", "thickness"));
        assert!(fits(">=120", "rl") && !fits(">120", "rl") && fits("> 100", "rl"), "text that reads as a number compares as one");
        assert!(fits("<-20, >= 0.2", "thickness"), "any item of the list");
        assert!(!fits("<5", "seam"), "a comparison never fits a word");
        let word = attrs(&[("note", AttrValue::Text("<abc".into()))]);
        assert!(list_fits("<abc", attribute_ci(&word, "note")), "`<abc` is no comparison: read as a word");
        assert!(!fits("<1", "missing"), "a value that isn't there fits no comparison");
    }

    #[test]
    fn not_takes_values_out_and_fits_what_is_not_there() {
        let coal = attrs(&[("material", AttrValue::Text("Coal".into()))]);
        let waste = attrs(&[("material", AttrValue::Text("WASTE".into()))]);
        let none = attrs(&[]);
        let fits = |list: &str, a: &Attributes| list_fits(list, attribute_ci(a, "material"));
        assert!(fits("!waste", &coal) && !fits("!waste", &waste) && fits("!waste", &none), "not waste, case aside; what has none isn't waste");
        assert!(fits("*, !WASTE", &coal) && !fits("*, !WASTE", &waste) && fits("*, !WASTE", &none));
        assert!(fits("Coal, Parting, !WASTE", &coal) && !fits("Coal, Parting, !WASTE", &none), "with values it may hold, one must be there");
        let strip = attrs(&[("strip", AttrValue::Number(4.0))]);
        assert!(!list_fits("1-10, !4", attribute_ci(&strip, "strip")) && list_fits("1-10, !5-6", attribute_ci(&strip, "strip")));
        assert!(list_fits("!", attribute_ci(&strip, "strip")), "a lone ! names nothing");
    }

    fn object(body: Body, attributes: Attributes) -> Object {
        Object { id: crate::ObjectId::new(), revision: 1, name: "test".into(), layer: crate::LayerId::new(), provenance: crate::RunId::new(), attributes, body }
    }

    /// A box at mine coordinates, `w` by `d` by `h`, its floor at RL `z0`, wound outward.
    fn block(w: f64, d: f64, h: f64, z0: f64) -> crate::TriMesh {
        let (x0, y0) = (650_000.0, 7_450_000.0);
        let (x1, y1, z1) = (x0 + w, y0 + d, z0 + h);
        let vertices = vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
        let triangles = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        crate::TriMesh::new(vertices, triangles)
    }

    #[test]
    fn measures_filter_as_attributes_do() {
        // A 10 m by 20 m by 2.5 m block, floor at RL 100: 500 m³.
        let solid = object(Body::Solid(crate::Solid::new(block(10.0, 20.0, 2.5, 100.0)).unwrap()), attrs(&[("seam", AttrValue::Text("U".into()))]));
        assert!((measure(&solid, "volume").unwrap() - 500.0).abs() < 1e-6, "volume of record, 1e-6 m³");
        assert_eq!((measure(&solid, "RL_min"), measure(&solid, "rl_max")), (Some(100.0), Some(102.5)), "name's case aside");
        assert_eq!(measure(&solid, "length"), None, "a solid has no length");
        let when = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>();
        assert!(object_fits(&when(&[("volume", "<=500"), ("seam", "U")]), &solid));
        assert!(!object_fits(&when(&[("volume", "<500")]), &solid), "500 m³ isn't under 500");
        assert!(object_fits(&when(&[("rl_max", ">102"), ("rl_min", "100")]), &solid));
        // An attribute of a measure's name is what the filter reads.
        let named = object(Body::Solid(crate::Solid::new(block(1.0, 1.0, 1.0, 0.0)).unwrap()), attrs(&[("volume", AttrValue::Number(9.0))]));
        assert!(object_fits(&when(&[("volume", "9")]), &named));

        // A 3-4-5 string climbing 12 m: 13 m long in 3D. Closed, a 30 m by 40 m rectangle in plan: 1,200 m².
        let x0 = 650_000.0;
        let open = object(Body::String(crate::Polyline { vertices: vec![[x0, 0.0, 100.0], [x0 + 3.0, 4.0, 112.0]], closed: false }), attrs(&[]));
        assert!((measure(&open, "length").unwrap() - 13.0).abs() < 1e-9 && measure(&open, "area").is_none());
        let ring = object(Body::String(crate::Polyline { vertices: vec![[x0, 0.0, 90.0], [x0 + 30.0, 0.0, 95.0], [x0 + 30.0, 40.0, 95.0], [x0, 40.0, 90.0]], closed: true }), attrs(&[]));
        assert!((measure(&ring, "area").unwrap() - 1200.0).abs() < 1e-6 && (measure(&ring, "length").unwrap() - 2.0 * (30.0_f64.hypot(5.0) + 40.0)).abs() < 1e-9);
        assert!(object_fits(&when(&[("area", ">1000"), ("length", "<200")]), &ring) && !object_fits(&when(&[("volume", "<1")]), &ring), "a measure it hasn't fits no value");

        // A surface: the block's top, 200 m²; a grid's RLs over the nodes that have one.
        let top = crate::TriMesh::new(vec![[x0, 0.0, 50.0], [x0 + 10.0, 0.0, 50.0], [x0 + 10.0, 20.0, 50.0], [x0, 20.0, 50.0]], vec![[0, 1, 2], [0, 2, 3]]);
        let surface = object(Body::Surface(crate::Surface::new(top)), attrs(&[]));
        assert!((measure(&surface, "area").unwrap() - 200.0).abs() < 1e-9 && measure(&surface, "rl_min") == Some(50.0));
        let grid = object(Body::Grid(std::sync::Arc::new(crate::Grid::new([0.0, 0.0], [10.0, 10.0], 2, 2, vec![f64::NAN, 70.0, 80.0, f64::NAN]))), attrs(&[]));
        assert_eq!((measure(&grid, "rl_min"), measure(&grid, "rl_max")), (Some(70.0), Some(80.0)));
    }

    #[test]
    fn a_workflow_input_reads_a_word_as_a_filter_and_a_number_as_itself() {
        let solid = object(Body::Solid(crate::Solid::new(block(10.0, 10.0, 10.0, 0.0)).unwrap()), attrs(&[("strip", AttrValue::Number(3.0)), ("material", AttrValue::Text("coal".into()))]));
        let filter = |pairs: Vec<(&str, AttrValue)>| pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>();
        assert!(input_filter_fits(&filter(vec![("strip", AttrValue::Number(3.0))]), &solid), "a number saved before, as itself");
        assert!(!input_filter_fits(&filter(vec![("strip", AttrValue::Number(4.0))]), &solid));
        assert!(input_filter_fits(&filter(vec![("strip", AttrValue::Text("1-5".into())), ("material", AttrValue::Text("Coal".into()))]), &solid));
        assert!(input_filter_fits(&filter(vec![("material", AttrValue::Text("!waste".into())), ("volume", AttrValue::Text(">=1000".into()))]), &solid));
        assert!(!input_filter_fits(&filter(vec![("volume", AttrValue::Text("<1000".into()))]), &solid));
    }
}

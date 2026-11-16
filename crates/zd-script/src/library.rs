//! The library a formula calls (decision 0077): the arithmetic every reserves dataflow needs, written once in Rust and
//! tested here, so a script says `in_situ_rd(unit.rd, unit.mad, unit.mis)` rather than retyping Preston and Sanders.
//! Moistures are percentages, as the fields hold them.

use starlark::environment::GlobalsBuilder;
use starlark::values::float::UnpackFloat;
use starlark::values::list::UnpackList;
use starlark::values::none::NoneOr;

/// In situ density from an air-dried RD and the air-dried and in situ moistures (%), by Preston and Sanders (1993):
/// RD × (100 − Mad) ÷ (100 + RD × (Mis − Mad) − Mis).
pub fn in_situ_rd(rd: f64, mad: f64, mis: f64) -> f64 {
    rd * (100.0 - mad) / (100.0 + rd * (mis - mad) - mis)
}

/// The air-dried RD back from an in situ density: Preston and Sanders with the moistures swapped.
pub fn air_dried_rd(rd_is: f64, mad: f64, mis: f64) -> f64 {
    in_situ_rd(rd_is, mis, mad)
}

/// What a quality or tonnage on one moisture basis is multiplied by to put it on another: (100 − to) ÷ (100 − from).
/// Air dried to in situ is `moisture_factor(mad, mis)`.
pub fn moisture_factor(from: f64, to: f64) -> f64 {
    (100.0 - to) / (100.0 - from)
}

/// A value on one moisture basis put on another.
pub fn to_basis(value: f64, from: f64, to: f64) -> f64 {
    value * moisture_factor(from, to)
}

/// An average weighted by the weights given (a composite by thickness, mass or mass × yield); none where the weights add
/// to nothing.
pub fn weighted(values: &[f64], weights: &[f64]) -> Option<f64> {
    let total: f64 = weights.iter().sum();
    if values.len() != weights.len() || total == 0.0 {
        return None;
    }
    Some(values.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>() / total)
}

/// The library as Starlark sees it, with the few maths functions Starlark leaves out.
#[starlark::starlark_module]
pub(crate) fn library(builder: &mut GlobalsBuilder) {
    /// In situ density from an air-dried RD, Mad and Mis (%), by Preston and Sanders.
    fn in_situ_rd(rd: UnpackFloat, mad: UnpackFloat, mis: UnpackFloat) -> anyhow::Result<f64> {
        Ok(crate::library::in_situ_rd(rd.0, mad.0, mis.0))
    }

    /// The air-dried RD back from an in situ density, Mad and Mis (%).
    fn air_dried_rd(rd_is: UnpackFloat, mad: UnpackFloat, mis: UnpackFloat) -> anyhow::Result<f64> {
        Ok(crate::library::air_dried_rd(rd_is.0, mad.0, mis.0))
    }

    /// (100 − to) ÷ (100 − from): a moisture basis to another.
    fn moisture_factor(from: UnpackFloat, to: UnpackFloat) -> anyhow::Result<f64> {
        Ok(crate::library::moisture_factor(from.0, to.0))
    }

    /// A value on the `from` moisture basis put on the `to` basis.
    fn to_basis(value: UnpackFloat, from: UnpackFloat, to: UnpackFloat) -> anyhow::Result<f64> {
        Ok(crate::library::to_basis(value.0, from.0, to.0))
    }

    /// An average weighted by `weights`; None where they add to nothing.
    fn weighted(values: UnpackList<UnpackFloat>, weights: UnpackList<UnpackFloat>) -> anyhow::Result<NoneOr<f64>> {
        let v: Vec<f64> = values.items.iter().map(|x| x.0).collect();
        let w: Vec<f64> = weights.items.iter().map(|x| x.0).collect();
        if v.len() != w.len() {
            anyhow::bail!("weighted: {} values but {} weights", v.len(), w.len());
        }
        Ok(match crate::library::weighted(&v, &w) {
            Some(x) => NoneOr::Other(x),
            None => NoneOr::None,
        })
    }

    /// The size of a number, never negative.
    fn abs(x: UnpackFloat) -> anyhow::Result<f64> {
        Ok(x.0.abs())
    }

    /// A number rounded to `digits` decimals (none: to a whole number).
    fn round(x: UnpackFloat, #[starlark(default = 0)] digits: i32) -> anyhow::Result<f64> {
        let scale = 10f64.powi(digits);
        Ok((x.0 * scale).round() / scale)
    }

    /// The square root; refused for a negative number.
    fn sqrt(x: UnpackFloat) -> anyhow::Result<f64> {
        if x.0 < 0.0 {
            anyhow::bail!("sqrt of a negative number ({})", x.0);
        }
        Ok(x.0.sqrt())
    }

    /// A number held between `low` and `high`.
    fn clamp(x: UnpackFloat, low: UnpackFloat, high: UnpackFloat) -> anyhow::Result<f64> {
        Ok(x.0.max(low.0).min(high.0))
    }
}

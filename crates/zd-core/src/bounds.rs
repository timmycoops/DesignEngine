/// Axis-aligned bounding box in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    /// Bounds of a set of points, or `None` when the set is empty.
    pub fn from_points<'a>(points: impl IntoIterator<Item = &'a [f64; 3]>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = *points.next()?;
        let mut bounds = Aabb { min: first, max: first };
        for p in points {
            for (axis, &v) in p.iter().enumerate() {
                bounds.min[axis] = bounds.min[axis].min(v);
                bounds.max[axis] = bounds.max[axis].max(v);
            }
        }
        Some(bounds)
    }

    pub fn centre(&self) -> [f64; 3] {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
            0.5 * (self.min[2] + self.max[2]),
        ]
    }

    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    /// True when `other` lies inside this box in plan (x and y only).
    pub fn contains_plan(&self, other: &Aabb) -> bool {
        self.min[0] <= other.min[0]
            && self.min[1] <= other.min[1]
            && self.max[0] >= other.max[0]
            && self.max[1] >= other.max[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_of_points() {
        let pts = [[1.0, 5.0, -2.0], [3.0, 2.0, 4.0]];
        let b = Aabb::from_points(&pts).unwrap();
        assert_eq!(b.min, [1.0, 2.0, -2.0]);
        assert_eq!(b.max, [3.0, 5.0, 4.0]);
        assert_eq!(b.centre(), [2.0, 3.5, 1.0]);
        assert!(Aabb::from_points(&[]).is_none());
    }
}

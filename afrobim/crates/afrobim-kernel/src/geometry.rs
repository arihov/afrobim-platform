use crate::Pt;

/// An axis-aligned box in model space. Always normalised: min ≤ max on both axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub min: Pt,
    pub max: Pt,
}

impl BBox {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self { min: Pt::new(min_x, min_y), max: Pt::new(max_x, max_y) }
    }

    pub fn from_corners(a: Pt, b: Pt) -> Self {
        Self::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn centre(&self) -> Pt {
        Pt::new((self.min.x + self.max.x) / 2.0, (self.min.y + self.max.y) / 2.0)
    }

    pub fn contains_point(&self, p: Pt) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// True when `o` lies wholly inside this box (window selection).
    pub fn contains_box(&self, o: &BBox) -> bool {
        o.min.x >= self.min.x && o.max.x <= self.max.x && o.min.y >= self.min.y && o.max.y <= self.max.y
    }

    /// True when the boxes touch or overlap (crossing selection).
    pub fn intersects(&self, o: &BBox) -> bool {
        o.min.x <= self.max.x && o.max.x >= self.min.x && o.min.y <= self.max.y && o.max.y >= self.min.y
    }

    pub fn inflate(&self, d: f64) -> BBox {
        BBox::new(self.min.x - d, self.min.y - d, self.max.x + d, self.max.y + d)
    }

    pub fn union(&self, o: &BBox) -> BBox {
        BBox::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y), self.max.x.max(o.max.x), self.max.y.max(o.max.y))
    }
}

//! Basic geometry types for layout and routing experiments.
//!
//! These are intentionally lightweight (no external dependencies) and provide
//! only the helpers we need for coarse grid placement and obstacle-aware
//! Manhattan routing.

use std::cmp::{max, min};

/// 2D point in canvas coordinates (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub fn right(&self) -> usize {
        self.x + self.width
    }

    #[inline]
    pub fn bottom(&self) -> usize {
        self.y + self.height
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    #[inline]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// Inflate the rectangle by `pad` on all sides, saturating at zero.
    pub fn inflate(&self, pad: usize) -> Self {
        let x = self.x.saturating_sub(pad);
        let y = self.y.saturating_sub(pad);
        let width = self.width + pad * 2;
        let height = self.height + pad * 2;
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Union of two rectangles. Empty rectangles are ignored.
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let x1 = min(self.x, other.x);
        let y1 = min(self.y, other.y);
        let x2 = max(self.right(), other.right());
        let y2 = max(self.bottom(), other.bottom());
        Self::new(x1, y1, x2 - x1, y2 - y1)
    }
}

/// Side of a box where a port can live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortSide {
    Top,
    Right,
    Bottom,
    Left,
}

/// Connection point anchored to a box with an offset along that side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Port {
    pub side: PortSide,
    pub offset: usize,
}

/// Axis-aligned segment between two points (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub from: Point,
    pub to: Point,
}

impl Segment {
    pub fn new(from: Point, to: Point) -> Self {
        Self { from, to }
    }
}

/// Routed path for an edge represented as consecutive axis-aligned segments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeRoute {
    pub segments: Vec<Segment>,
}

impl EdgeRoute {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn push_segment(&mut self, from: Point, to: Point) {
        if from == to {
            return;
        }
        self.segments.push(Segment::new(from, to));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_union_and_inflate() {
        let r1 = Rect::new(2, 2, 3, 3);
        let r2 = Rect::new(5, 1, 2, 2);
        let union = r1.union(&r2);
        assert_eq!(union, Rect::new(2, 1, 5, 4));

        let inflated = r1.inflate(1);
        assert_eq!(inflated, Rect::new(1, 1, 5, 5));
    }
}

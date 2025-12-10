//! Direction-agnostic orientation system for diagram rendering.
//!
//! Provides abstractions for all four diagram orientations (TD, LR, BT, RL)
//! using a unified coordinate system.
//!
//! # Concepts
//!
//! - **Primary axis**: Flow direction (vertical for TD/BT, horizontal for LR/RL)
//! - **Secondary axis**: Branching direction (perpendicular to primary)
//! - **Advance/Retreat**: Movement along primary axis in flow direction

use crate::graph::Direction;
use crate::style::StyleChars;

/// Represents the two axes in a 2D layout
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Orientation-aware coordinate system
pub struct OrientedCoords {
    pub primary: Axis,
    pub secondary: Axis,
    pub direction: Direction,
}

impl OrientedCoords {
    /// Create a new oriented coordinate system based on diagram direction
    pub fn new(direction: Direction) -> Self {
        match direction {
            Direction::TD | Direction::TB => Self {
                primary: Axis::Vertical,
                secondary: Axis::Horizontal,
                direction,
            },
            Direction::LR => Self {
                primary: Axis::Horizontal,
                secondary: Axis::Vertical,
                direction,
            },
            Direction::RL => Self {
                primary: Axis::Horizontal,
                secondary: Axis::Vertical,
                direction,
            },
            Direction::BT => Self {
                primary: Axis::Vertical,
                secondary: Axis::Horizontal,
                direction,
            },
        }
    }

    /// Get the primary axis value from x,y coordinates
    pub fn primary_coord(&self, x: usize, y: usize) -> usize {
        match self.primary {
            Axis::Horizontal => x,
            Axis::Vertical => y,
        }
    }

    /// Get the secondary axis value from x,y coordinates
    pub fn secondary_coord(&self, x: usize, y: usize) -> usize {
        match self.secondary {
            Axis::Horizontal => x,
            Axis::Vertical => y,
        }
    }

    /// Set a coordinate on the primary axis
    pub fn set_primary(&self, x: &mut usize, y: &mut usize, value: usize) {
        match self.primary {
            Axis::Horizontal => *x = value,
            Axis::Vertical => *y = value,
        }
    }

    /// Set a coordinate on the secondary axis
    pub fn set_secondary(&self, x: &mut usize, y: &mut usize, value: usize) {
        match self.secondary {
            Axis::Horizontal => *x = value,
            Axis::Vertical => *y = value,
        }
    }

    /// Get the appropriate edge character for the primary axis
    pub fn primary_edge_char(&self, style: &StyleChars) -> char {
        match self.primary {
            Axis::Horizontal => style.edge_h,
            Axis::Vertical => style.edge_v,
        }
    }

    /// Get the appropriate edge character for the secondary axis
    pub fn secondary_edge_char(&self, style: &StyleChars) -> char {
        match self.secondary {
            Axis::Horizontal => style.edge_h,
            Axis::Vertical => style.edge_v,
        }
    }

    /// Get the appropriate arrow character for the end of flow
    pub fn arrow_end(&self, style: &StyleChars) -> char {
        match self.direction {
            Direction::TD | Direction::TB => style.arrow_down,
            Direction::LR => style.arrow_right,
            Direction::RL => style.arrow_left,
            Direction::BT => style.arrow_up,
        }
    }

    /// Get the appropriate junction character for a branch point
    /// where flow splits from primary to secondary axis
    pub fn junction_branch(&self, style: &StyleChars) -> char {
        match self.direction {
            Direction::TD | Direction::TB => style.junction_down,  // ┬ (branches down)
            Direction::LR => style.junction_right,                 // ├ (branches right)
            Direction::RL => style.junction_left,                  // ┤ (branches left)
            Direction::BT => style.junction_up,                    // ┴ (branches up)
        }
    }

    /// Get the appropriate junction character for a merge point
    /// where flow converges from secondary to primary axis
    pub fn junction_merge(&self, style: &StyleChars) -> char {
        match self.direction {
            Direction::TD | Direction::TB => style.junction_up,    // ┴ (merges up)
            Direction::LR => style.junction_left,                  // ┤ (merges left)
            Direction::RL => style.junction_right,                 // ├ (merges right)
            Direction::BT => style.junction_down,                  // ┬ (merges down)
        }
    }

    /// Get corner character for turning from primary to secondary axis
    /// at the start position
    pub fn corner_start_to_secondary(&self, going_before: bool, style: &StyleChars) -> char {
        match (self.direction, going_before) {
            (Direction::TD | Direction::TB, true) => style.corner_ul,   // ┌ (down to left)
            (Direction::TD | Direction::TB, false) => style.corner_ur,  // ┐ (down to right)
            (Direction::LR, true) => style.corner_dl,                   // └ (right to up)
            (Direction::LR, false) => style.corner_ul,                  // ┌ (right to down)
            (Direction::RL, true) => style.corner_dr,                   // ┘ (left to up)
            (Direction::RL, false) => style.corner_ur,                  // ┐ (left to down)
            (Direction::BT, true) => style.corner_dl,                   // └ (up to left)
            (Direction::BT, false) => style.corner_dr,                  // ┘ (up to right)
        }
    }

    /// Get corner character for turning from secondary to primary axis
    /// at the end position
    pub fn corner_secondary_to_end(&self, coming_from_before: bool, style: &StyleChars) -> char {
        match (self.direction, coming_from_before) {
            (Direction::TD | Direction::TB, true) => style.corner_dr,   // ┘ (left to down)
            (Direction::TD | Direction::TB, false) => style.corner_dl,  // └ (right to down)
            (Direction::LR, true) => style.corner_ur,                   // ┐ (up to right)
            (Direction::LR, false) => style.corner_dr,                  // ┘ (down to right)
            (Direction::RL, true) => style.corner_ul,                   // ┌ (up to left)
            (Direction::RL, false) => style.corner_dl,                  // └ (down to left)
            (Direction::BT, true) => style.corner_ur,                   // ┐ (left to up)
            (Direction::BT, false) => style.corner_ul,                  // ┌ (right to up)
        }
    }

    /// Advance position along primary axis in flow direction.
    pub fn advance(&self, x: usize, y: usize, distance: usize) -> (usize, usize) {
        let mut new_x = x;
        let mut new_y = y;

        match self.primary {
            Axis::Horizontal => {
                match self.direction {
                    Direction::RL => new_x = new_x.saturating_sub(distance),
                    _ => new_x += distance,
                }
            }
            Axis::Vertical => {
                match self.direction {
                    Direction::BT => new_y = new_y.saturating_sub(distance),
                    _ => new_y += distance,
                }
            }
        }

        (new_x, new_y)
    }

    /// Retreat position along primary axis (opposite of flow direction).
    pub fn retreat(&self, x: usize, y: usize, distance: usize) -> (usize, usize) {
        let mut new_x = x;
        let mut new_y = y;

        match self.primary {
            Axis::Horizontal => {
                match self.direction {
                    Direction::RL => new_x += distance,
                    _ => new_x = new_x.saturating_sub(distance),
                }
            }
            Axis::Vertical => {
                match self.direction {
                    Direction::BT => new_y += distance,
                    _ => new_y = new_y.saturating_sub(distance),
                }
            }
        }

        (new_x, new_y)
    }

    /// Return new coordinates with a specific secondary axis value.
    pub fn with_secondary(&self, x: usize, y: usize, secondary_val: usize) -> (usize, usize) {
        let mut new_x = x;
        let mut new_y = y;
        self.set_secondary(&mut new_x, &mut new_y, secondary_val);
        (new_x, new_y)
    }
}

/// Helper to determine if we're moving "before" or "after" on secondary axis
pub fn is_before(from: usize, to: usize) -> bool {
    from > to
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_td_orientation() {
        let coords = OrientedCoords::new(Direction::TD);
        assert_eq!(coords.primary, Axis::Vertical);
        assert_eq!(coords.secondary, Axis::Horizontal);
        
        // Primary coord should return y for TD
        assert_eq!(coords.primary_coord(10, 20), 20);
        // Secondary coord should return x for TD
        assert_eq!(coords.secondary_coord(10, 20), 10);
    }

    #[test]
    fn test_lr_orientation() {
        let coords = OrientedCoords::new(Direction::LR);
        assert_eq!(coords.primary, Axis::Horizontal);
        assert_eq!(coords.secondary, Axis::Vertical);
        
        // Primary coord should return x for LR
        assert_eq!(coords.primary_coord(10, 20), 10);
        // Secondary coord should return y for LR
        assert_eq!(coords.secondary_coord(10, 20), 20);
    }
}
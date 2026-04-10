//! Shared terminal display profile.
//!
//! This module centralizes the user-visible width and grapheme policy used by
//! measurement, wrapping, truncation, preview framing, and cursor math.

use unicode_segmentation::{Graphemes, UnicodeSegmentation};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Grapheme segmentation policy used for user-visible text slicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphemePolicy {
    Extended,
}

/// Display-width policy used for terminal cell budgeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthPolicy {
    UnicodeWidth,
}

/// Explicit display profile shared across renderer-adjacent text math.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayProfile {
    pub name: &'static str,
    pub grapheme_policy: GraphemePolicy,
    pub width_policy: WidthPolicy,
}

/// Default profile used across the current renderer and TUI surfaces.
pub const DEFAULT_DISPLAY_PROFILE: DisplayProfile = DisplayProfile {
    name: "unicode-width+extended-graphemes",
    grapheme_policy: GraphemePolicy::Extended,
    width_policy: WidthPolicy::UnicodeWidth,
};

impl DisplayProfile {
    pub fn graphemes<'a>(&self, text: &'a str) -> Graphemes<'a> {
        match self.grapheme_policy {
            GraphemePolicy::Extended => UnicodeSegmentation::graphemes(text, true),
        }
    }

    pub fn display_width(&self, text: &str) -> usize {
        match self.width_policy {
            WidthPolicy::UnicodeWidth => text.width(),
        }
    }

    pub fn char_width(&self, ch: char) -> usize {
        match self.width_policy {
            WidthPolicy::UnicodeWidth => UnicodeWidthChar::width(ch).unwrap_or(1),
        }
    }

    /// Return the longest grapheme-safe prefix that fits within `max_width`.
    pub fn truncate_to_width(&self, text: &str, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }

        let mut result = String::new();
        let mut width = 0usize;

        for grapheme in self.graphemes(text) {
            let grapheme_width = self.display_width(grapheme);
            if width + grapheme_width > max_width {
                break;
            }
            result.push_str(grapheme);
            width += grapheme_width;
        }

        result
    }

    /// Hard-wrap a string into grapheme-safe chunks of at most `max_width`.
    pub fn split_text_to_width_chunks(&self, text: &str, max_width: usize) -> Vec<String> {
        if max_width == 0 {
            return vec![String::new()];
        }
        if text.is_empty() {
            return vec![String::new()];
        }
        if self.display_width(text) <= max_width {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;

        for grapheme in self.graphemes(text) {
            let grapheme_width = self.display_width(grapheme);

            if grapheme_width == 0 {
                current.push_str(grapheme);
                continue;
            }

            if !current.is_empty() && current_width + grapheme_width > max_width {
                chunks.push(std::mem::take(&mut current));
                current_width = 0;
            }

            current.push_str(grapheme);
            if grapheme_width > max_width {
                chunks.push(std::mem::take(&mut current));
                current_width = 0;
                continue;
            }

            current_width += grapheme_width;
            if current_width >= max_width {
                chunks.push(std::mem::take(&mut current));
                current_width = 0;
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }
        if chunks.is_empty() {
            chunks.push(String::new());
        }

        chunks
    }
}

pub fn graphemes(text: &str) -> Graphemes<'_> {
    DEFAULT_DISPLAY_PROFILE.graphemes(text)
}

pub fn display_width(text: &str) -> usize {
    DEFAULT_DISPLAY_PROFILE.display_width(text)
}

pub fn display_char_width(ch: char) -> usize {
    DEFAULT_DISPLAY_PROFILE.char_width(ch)
}

pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    DEFAULT_DISPLAY_PROFILE.truncate_to_width(text, max_width)
}

pub fn split_text_to_width_chunks(text: &str, max_width: usize) -> Vec<String> {
    DEFAULT_DISPLAY_PROFILE.split_text_to_width_chunks(text, max_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_name_is_explicit() {
        assert_eq!(
            DEFAULT_DISPLAY_PROFILE.name,
            "unicode-width+extended-graphemes"
        );
    }

    #[test]
    fn display_profile_preserves_grapheme_clusters() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            truncate_to_width(&format!("{family}{family}"), display_width(family)),
            family
        );
    }

    #[test]
    fn display_profile_splits_chunks_by_width_not_bytes() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            split_text_to_width_chunks(&format!("{family}{family}"), display_width(family)),
            vec![family.to_string(), family.to_string()]
        );
    }

    #[test]
    fn display_profile_char_width_matches_renderer_expectation() {
        assert_eq!(display_char_width('A'), 1);
        assert_eq!(display_char_width('語'), 2);
    }
}

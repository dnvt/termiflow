//! Whole-line edge statement tokenizer.
//!
//! Parses `group (op [|label|] group)+` where `group = node (& node)*` and a
//! node is an identifier with an optional inline shape (`A[Label]`, `B{x}`,
//! …). Every supported edge kind accepts both pipe labels (`-->|t|`) and
//! text labels (`-- t -->`, `== t ==>`, `-. t .->`, `<-- t -->`,
//! `-- t ---`, `-- t --o`, `-- t --x`), so chains, `&` groups, and labels
//! compose uniformly instead of through one regex per edge kind.
//!
//! The tokenizer is strict: it returns `None` unless the entire line is one
//! edge statement, leaving every other line to the existing parser paths.

use crate::graph::EdgeKind;

/// One hop of an edge statement: every node in `from` connects to every node
/// in `to` with `kind` and `label`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct EdgeHop {
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub kind: EdgeKind,
    pub label: Option<String>,
}

/// Parse a complete edge statement. Returns the hops in source order, or
/// `None` when the line is not exactly one edge statement.
pub(super) fn parse_edge_statement(line: &str) -> Option<Vec<EdgeHop>> {
    let mut cursor = Cursor::new(line.trim().trim_end_matches(';').trim_end());
    let mut groups = vec![cursor.group()?];
    let mut ops = Vec::new();
    loop {
        cursor.skip_ws();
        if cursor.at_end() {
            break;
        }
        ops.push(cursor.operator()?);
        groups.push(cursor.group()?);
    }
    if ops.is_empty() {
        return None;
    }
    Some(
        ops.into_iter()
            .enumerate()
            .map(|(index, (kind, label))| EdgeHop {
                from: groups[index].clone(),
                to: groups[index + 1].clone(),
                kind,
                label,
            })
            .collect(),
    )
}

/// Strip one pair of surrounding double quotes from a label.
pub(super) fn unquote_label(label: &str) -> &str {
    let trimmed = label.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
}

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    fn skip_ws(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn group(&mut self) -> Option<Vec<String>> {
        let mut ids = vec![self.node()?];
        loop {
            let saved = self.pos;
            self.skip_ws();
            if self.eat("&") {
                self.skip_ws();
                ids.push(self.node()?);
            } else {
                self.pos = saved;
                return Some(ids);
            }
        }
    }

    fn node(&mut self) -> Option<String> {
        self.skip_ws();
        let rest = self.rest();
        let id_len = rest
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(rest.len(), |(index, _)| index);
        if id_len == 0 {
            return None;
        }
        let id = rest[..id_len].to_string();
        self.pos += id_len;
        self.shape()?;
        Some(id)
    }

    /// Consume an optional inline shape after a node id. Shape semantics are
    /// recorded by the node-definition pass; here we only need to skip it.
    fn shape(&mut self) -> Option<()> {
        const SHAPES: &[(&str, &[&str])] = &[
            ("(((", &[")))"]),
            ("((", &["))"]),
            ("([", &["])"]),
            ("(", &[")"]),
            ("[[", &["]]"]),
            ("[(", &[")]"]),
            ("[/", &["/]", "\\]"]),
            ("[\\", &["\\]", "/]"]),
            ("[", &["]"]),
            ("{{", &["}}"]),
            ("{", &["}"]),
            (">", &["]"]),
        ];
        let rest = self.rest();
        let Some((open, closers)) = SHAPES.iter().find(|(open, _)| rest.starts_with(open)) else {
            return Some(());
        };
        let body_start = open.len();
        let body = &rest[body_start..];
        // A quoted label may contain shape delimiters; skip past it first.
        let search_from = match body.strip_prefix('"') {
            Some(quoted) => quoted.find('"').map(|end| end + 2)?,
            None => 0,
        };
        let close = closers
            .iter()
            .filter_map(|closer| {
                body[search_from..]
                    .find(closer)
                    .map(|at| (search_from + at, closer.len()))
            })
            .min_by_key(|(at, _)| *at)?;
        self.pos += body_start + close.0 + close.1;
        Some(())
    }

    fn operator(&mut self) -> Option<(EdgeKind, Option<String>)> {
        let rest = self.rest();
        let (kind, text_label) = if let Some(after) = rest.strip_prefix('<') {
            let dashes = run_len(after, '-');
            if dashes >= 2 && after[dashes..].starts_with('>') {
                self.pos += 1 + dashes + 1;
                (EdgeKind::Bidirectional, None)
            } else if dashes == 2 && starts_with_ws(&after[dashes..]) {
                self.pos += 1 + dashes;
                let label = self.text_label_until(|s| arrow_closer(s, '-', '>'))?;
                (EdgeKind::Bidirectional, Some(label))
            } else {
                return None;
            }
        } else if rest.starts_with('=') {
            let equals = run_len(rest, '=');
            if equals >= 2 && rest[equals..].starts_with('>') {
                self.pos += equals + 1;
                (EdgeKind::Thick, None)
            } else if equals == 2 && starts_with_ws(&rest[equals..]) {
                self.pos += equals;
                let label = self.text_label_until(|s| arrow_closer(s, '=', '>'))?;
                (EdgeKind::Thick, Some(label))
            } else {
                return None;
            }
        } else if let Some(after) = rest.strip_prefix("-.") {
            let dots = run_len(after, '.');
            if after[dots..].starts_with("->") {
                self.pos += 2 + dots + 2;
                (EdgeKind::Dotted, None)
            } else if dots == 0 && starts_with_ws(after) {
                self.pos += 2;
                let label = self.text_label_until(dotted_closer)?;
                (EdgeKind::Dotted, Some(label))
            } else {
                return None;
            }
        } else if rest.starts_with('-') {
            let dashes = run_len(rest, '-');
            let after = &rest[dashes..];
            if dashes >= 2 && after.starts_with('>') {
                self.pos += dashes + 1;
                (EdgeKind::Arrow, None)
            } else if dashes >= 2 && end_marker(after).is_some() {
                self.pos += dashes + 1;
                (end_marker(after)?, None)
            } else if dashes >= 3 {
                self.pos += dashes;
                (EdgeKind::Open, None)
            } else if dashes == 2 && starts_with_ws(after) {
                self.pos += dashes;
                let (label, kind) = self.text_label_with_dash_closer()?;
                (kind, Some(label))
            } else {
                return None;
            }
        } else {
            return None;
        };

        if text_label.is_some() {
            return Some((kind, text_label.filter(|label| !label.is_empty())));
        }
        // Optional pipe label directly after the operator.
        let saved = self.pos;
        self.skip_ws();
        if self.eat("|") {
            let rest = self.rest();
            let end = rest.find('|')?;
            let label = unquote_label(&rest[..end]).to_string();
            self.pos += end + 1;
            return Some((kind, (!label.is_empty()).then_some(label)));
        }
        self.pos = saved;
        Some((kind, None))
    }

    /// Read a text label up to a closing operator located by `closer`, which
    /// returns `(start, len)` of the closer within the remaining text.
    fn text_label_until(
        &mut self,
        closer: impl Fn(&str) -> Option<(usize, usize)>,
    ) -> Option<String> {
        let rest = self.rest();
        let (start, len) = closer(rest)?;
        let label = unquote_label(&rest[..start]).to_string();
        self.pos += start + len;
        Some(label)
    }

    /// `-- text -->`, `-- text ---`, `-- text --o`, `-- text --x`.
    fn text_label_with_dash_closer(&mut self) -> Option<(String, EdgeKind)> {
        let rest = self.rest();
        let mut search = 0;
        while let Some(found) = rest[search..].find("--") {
            let start = search + found;
            if !rest[..start].ends_with(char::is_whitespace) {
                search = start + 2;
                continue;
            }
            let dashes = run_len(&rest[start..], '-');
            let after = &rest[start + dashes..];
            let kind = if after.starts_with('>') {
                Some((EdgeKind::Arrow, dashes + 1))
            } else if let Some(kind) = end_marker(after) {
                Some((kind, dashes + 1))
            } else if dashes >= 3 {
                Some((EdgeKind::Open, dashes))
            } else {
                None
            };
            if let Some((kind, len)) = kind {
                let label = unquote_label(&rest[..start]).to_string();
                self.pos += start + len;
                return Some((label, kind));
            }
            search = start + dashes;
        }
        None
    }
}

fn run_len(text: &str, ch: char) -> usize {
    text.chars().take_while(|c| *c == ch).count() * ch.len_utf8()
}

fn starts_with_ws(text: &str) -> bool {
    text.starts_with(char::is_whitespace)
}

/// `o`/`x` end markers count only when they are not the start of an id.
fn end_marker(after: &str) -> Option<EdgeKind> {
    let kind = match after.chars().next()? {
        'o' => EdgeKind::CircleEnd,
        'x' => EdgeKind::CrossEnd,
        _ => return None,
    };
    let next = after[1..].chars().next();
    match next {
        None => Some(kind),
        Some(c) if c.is_whitespace() || c == '|' => Some(kind),
        _ => None,
    }
}

/// Find ` <ch><ch>+<tail>` preceded by whitespace (e.g. ` -->`, ` ==>`).
fn arrow_closer(text: &str, ch: char, tail: char) -> Option<(usize, usize)> {
    let pair: String = [ch, ch].iter().collect();
    let mut search = 0;
    while let Some(found) = text[search..].find(&pair) {
        let start = search + found;
        let run = run_len(&text[start..], ch);
        if text[..start].ends_with(char::is_whitespace) && text[start + run..].starts_with(tail) {
            return Some((start, run + tail.len_utf8()));
        }
        search = start + run;
    }
    None
}

/// Find ` .->` / ` ..->` preceded by whitespace.
fn dotted_closer(text: &str) -> Option<(usize, usize)> {
    let mut search = 0;
    while let Some(found) = text[search..].find('.') {
        let start = search + found;
        let dots = run_len(&text[start..], '.');
        if text[..start].ends_with(char::is_whitespace) && text[start + dots..].starts_with("->") {
            return Some((start, dots + 2));
        }
        search = start + dots;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    type Hop = (Vec<String>, Vec<String>, EdgeKind, Option<String>);

    fn hops(line: &str) -> Vec<Hop> {
        parse_edge_statement(line)
            .unwrap_or_else(|| panic!("not a statement: {line}"))
            .into_iter()
            .map(|hop| (hop.from, hop.to, hop.kind, hop.label))
            .collect()
    }

    fn hop(from: &[&str], to: &[&str], kind: EdgeKind, label: Option<&str>) -> Hop {
        (
            from.iter().map(|s| s.to_string()).collect(),
            to.iter().map(|s| s.to_string()).collect(),
            kind,
            label.map(str::to_string),
        )
    }

    #[test]
    fn parses_every_operator_form() {
        for (line, kind, label) in [
            ("A --> B", EdgeKind::Arrow, None),
            ("A ---> B", EdgeKind::Arrow, None),
            ("A --- B", EdgeKind::Open, None),
            ("A ==> B", EdgeKind::Thick, None),
            ("A -.-> B", EdgeKind::Dotted, None),
            ("A <--> B", EdgeKind::Bidirectional, None),
            ("A --o B", EdgeKind::CircleEnd, None),
            ("A --x B", EdgeKind::CrossEnd, None),
            ("A -->|go| B", EdgeKind::Arrow, Some("go")),
            ("A ==>|go| B", EdgeKind::Thick, Some("go")),
            ("A -- go --> B", EdgeKind::Arrow, Some("go")),
            ("A == go ==> B", EdgeKind::Thick, Some("go")),
            ("A -. go .-> B", EdgeKind::Dotted, Some("go")),
            ("A <-- go --> B", EdgeKind::Bidirectional, Some("go")),
            ("A -- go --- B", EdgeKind::Open, Some("go")),
            ("A -- go --o B", EdgeKind::CircleEnd, Some("go")),
            ("A -- go --x B", EdgeKind::CrossEnd, Some("go")),
            ("A -- two words --> B", EdgeKind::Arrow, Some("two words")),
        ] {
            assert_eq!(hops(line), vec![hop(&["A"], &["B"], kind, label)], "{line}");
        }
    }

    #[test]
    fn end_marker_does_not_eat_an_id_prefix() {
        assert_eq!(
            hops("A --- ox"),
            vec![hop(&["A"], &["ox"], EdgeKind::Open, None)]
        );
    }

    #[test]
    fn rejects_non_statements() {
        for line in [
            "A[Label]",
            "subgraph X",
            "A",
            "A -->",
            "--> B",
            "A --> B extra words",
        ] {
            assert!(parse_edge_statement(line).is_none(), "{line}");
        }
    }

    #[test]
    fn shapes_with_delimiters_inside_quotes_are_skipped() {
        assert_eq!(
            hops("A[\"x ] y\"] --> B{{h}}"),
            vec![hop(&["A"], &["B"], EdgeKind::Arrow, None)]
        );
    }
}

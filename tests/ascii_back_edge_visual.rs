use termiflow::{BaseStyle, RenderOptions};

fn ascii(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
        .expect("render")
        .output
}

/// A solid back edge is drawn solid in ASCII, as in Unicode: its vertical
/// legs use `|`, not the dotted `:`.
#[test]
fn an_ascii_solid_back_edge_is_drawn_solid() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let frame = ascii(&format!("graph {direction}\nA --> B\nB --> A\n"));
        assert!(
            !frame.contains(':'),
            "solid back edge drawn dotted ({direction}):\n{frame}"
        );
    }
}

/// A dotted forward edge keeps its dotted ASCII glyph.
#[test]
fn an_ascii_dotted_edge_stays_dotted() {
    let frame = ascii("graph TD\nA -.-> B\n");
    assert!(frame.contains(':'), "{frame}");
}

/// A box side tee that also turns an edge keeps its ASCII joint: the `+`
/// where `P`'s right wall meets the edge leaving it is not redrawn `|`.
#[test]
fn an_ascii_box_tee_keeps_its_edge_turn() {
    let frame = ascii(
        "graph LR\nX[Ext1] & Y[Ext2] --> P & Q\nsubgraph G [Group]\nP[P]\nQ[Q]\nend\nP & Q --> Z[Sink]\n",
    );
    for name in ["P", "Q"] {
        let row = frame
            .lines()
            .find(|line| line.contains(&format!("+  {name}  +")))
            .unwrap_or_else(|| panic!("no {name} row:\n{frame}"));
        assert!(
            row.contains(&format!("+  {name}  ++")),
            "{name}'s exit turn drawn as a wall:\n{frame}"
        );
    }
}

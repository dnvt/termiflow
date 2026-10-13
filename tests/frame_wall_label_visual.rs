use termiflow::{BaseStyle, RenderOptions};

/// A label on an edge leaving a frame sideways sits clear of the frame wall:
/// the wall's column on the label row stays part of the edge line.
#[test]
fn an_edge_label_does_not_overwrite_a_frame_wall() {
    for (direction, label_side) in [("LR", "→"), ("RL", "←")] {
        for optimized in [false, true] {
            let input = format!(
                "graph {direction}\n    subgraph S [Auth Flow]\n        A[Login] --> B[Check]\n    end\n    B -->|success| C[Out]\n"
            );
            let frame = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(BaseStyle::Unicode)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let row = lines
                .iter()
                .find(|line| line.iter().collect::<String>().contains("success"))
                .unwrap_or_else(|| panic!("label row is missing:\n{frame}"));
            let walls: Vec<usize> = lines
                .iter()
                .find(|line| line.iter().collect::<String>().contains("Auth Flow"))
                .map(|line| {
                    line.iter()
                        .enumerate()
                        .filter(|(_, ch)| matches!(**ch, '│' | '┃'))
                        .map(|(x, _)| x)
                        .collect()
                })
                .unwrap_or_default();
            assert!(!walls.is_empty(), "{frame}");
            for x in walls {
                let ch = row.get(x).copied().unwrap_or(' ');
                assert!(
                    !ch.is_alphanumeric(),
                    "the label covers the frame wall at column {x} \
                     ({direction} {label_side} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}

const ISTIO_MESH_BT: &str = "graph BT
    subgraph Mesh [Istio Mesh]
        subgraph NSa [ns: storefront]
            FE[frontend]
            FEp{{envoy}}
            FE --- FEp
        end
        subgraph NSb [ns: catalog]
            Cat[catalog]
            Catp{{envoy}}
            Cat --- Catp
        end
        FEp ==>|mTLS| Catp
    end
    IGW[Ingress Gateway] --> FEp
    Istiod[istiod] -.-> FEp
    Istiod -.-> Catp
";

/// A BT edge merging into a target with others keeps its label on its own
/// run where its stem turns toward the target, not in empty space above
/// its source; and a frame title crossed by a line slides clear of it, so
/// the title stays whole and no letter sits on the line's column.
#[test]
fn a_merging_bt_label_stays_on_its_run_and_a_title_clears_a_line() {
    for (style, bus_label, line) in [
        (BaseStyle::Unicode, "─mTLS─", '│'),
        (BaseStyle::Ascii, "-mTLS-", '|'),
    ] {
        let frame =
            termiflow::render_with_feedback(ISTIO_MESH_BT, RenderOptions::new().with_style(style))
                .expect("render")
                .output;
        assert!(
            frame.contains(bus_label),
            "mTLS sits on its edge's run ({style:?}):\n{frame}"
        );
        let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
        let title_row = rows
            .iter()
            .position(|row| row.iter().collect::<String>().contains("ns: catalog"))
            .unwrap_or_else(|| panic!("whole catalog title ({style:?}):\n{frame}"));
        // A line running up through the title band (above the title row and
        // below the frame's bottom border) keeps its column free of letters.
        let cell = |r: usize, c: usize| rows.get(r).and_then(|row| row.get(c)).copied();
        let crossings: Vec<usize> = (0..rows[title_row].len())
            .filter(|&c| {
                cell(title_row - 1, c) == Some(line) && cell(title_row + 2, c) == Some(line)
            })
            .collect();
        assert!(
            !crossings.is_empty()
                && crossings
                    .iter()
                    .all(|&c| matches!(cell(title_row, c), Some(' '))
                        || cell(title_row, c) == Some(line)),
            "no title letter covers the line through the title band ({style:?}):\n{frame}"
        );
    }
}

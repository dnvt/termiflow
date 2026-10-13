use termiflow::{BaseStyle, RenderOptions};

const BT_EXTERNAL_FANOUT_INTO_TITLED_SUBGRAPH: &str = "graph BT
    Root[Dispatcher]
    subgraph Workers [Workers]
        W1[One]
        W2[Two]
        W3[Three]
    end
    Root --> W1
    Root --> W2
    Root --> W3
";

#[test]
fn bt_external_fanout_trunk_reaches_the_bus_inside_a_titled_subgraph() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                BT_EXTERNAL_FANOUT_INTO_TITLED_SUBGRAPH,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let source_row = rows
                .iter()
                .position(|row| row.iter().collect::<String>().contains("Dispatcher"))
                .expect("source label");
            // The trunk leaves the source's top border through its junction.
            let top_border = &rows[source_row - 1];
            let trunk_x = (0..top_border.len())
                .find(|&x| {
                    matches!(top_border[x], '┴' | '+')
                        && matches!(rows[source_row - 2].get(x), Some('│' | '|'))
                })
                .unwrap_or_else(|| panic!("source exit junction:\n{frame}"));
            let mut y = source_row - 2;
            while y > 0 && matches!(rows[y].get(trunk_x), Some('│' | '|')) {
                y -= 1;
            }
            let end = rows[y].get(trunk_x).copied().unwrap_or(' ');
            assert!(
                matches!(end, '┬' | '+')
                    && rows[y].iter().filter(|g| matches!(g, '↑' | '^')).count() == 0,
                "trunk must climb through the border and title row into the bus junction, \
                 stopped at row {y} on {end:?} ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

const TD_EXTERNAL_FANOUT_WITH_SHALLOW_EXIT: &str = "graph TD
    GW[API Gateway]
    subgraph Services [Core Services]
        Auth[Auth Service]
        Orders[Order Service]
        Catalog[Catalog Service]
    end
    GW --> Auth
    GW --> Orders
    GW --> Catalog
    Orders --> DB[(Orders DB)]
";

#[test]
fn td_external_fanout_keeps_its_bus_row_when_an_exit_target_sits_close_below() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                TD_EXTERNAL_FANOUT_WITH_SHALLOW_EXIT,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let arrow_row = rows
                .iter()
                .position(|row| row.iter().filter(|g| matches!(g, '↓' | 'v')).count() == 3)
                .unwrap_or_else(|| panic!("one row carries the three entry arrows:\n{frame}"));
            let arrows: Vec<usize> = (0..rows[arrow_row].len())
                .filter(|&x| matches!(rows[arrow_row][x], '↓' | 'v'))
                .collect();
            let between_arrows_is_quiet =
                (arrows[0]..=arrows[2]).all(|x| matches!(rows[arrow_row][x], '↓' | 'v' | ' '));
            let bus_feeds_every_arrow = arrows.iter().all(|&x| {
                matches!(
                    rows[arrow_row - 1].get(x),
                    Some('┌' | '┐' | '┬' | '┼' | '│' | '+' | '|')
                )
            });
            assert!(
                between_arrows_is_quiet && bus_feeds_every_arrow,
                "the fan-out bus must sit on its own row above the entry arrows \
                 ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// An LR fan-out whose source row lies above every target (the subgraph
/// holding the source pushes its exit past them) still joins its bus: the
/// source's stub turns down into it instead of ending in mid-air.
#[test]
fn a_fan_out_from_beyond_its_targets_joins_its_bus() {
    let input = "flowchart LR
    subgraph Device [Edge Device]
        Camera[Camera]
        Detect[On-device Detector]
        Camera --> Detect
    end
    subgraph Cloud
        Upload[Upload Clips]
        Verify[Cloud Verifier]
        Upload --> Verify
    end
    Detect --> Upload
    Verify --> Alert>Push Alert]
    Verify --> Archive[(Clip Archive)]
";
    for (style, turn) in [(BaseStyle::Unicode, '┐'), (BaseStyle::Ascii, '+')] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let source_row = frame
            .lines()
            .find(|line| line.contains("Cloud Verifier"))
            .expect("source row");
        assert_eq!(
            source_row.trim_end().chars().last(),
            Some(turn),
            "Verify's fan-out stub does not reach its bus ({style:?}):\n{frame}"
        );
    }
}

/// A TD fan-out whose middle target, with no edge of its own leaving it,
/// sits one column off the source stem drops into it on the stem's column,
/// so the bus shows one junction rather than two side by side (`┴┬`, ASCII
/// `++`).
#[test]
fn a_near_miss_fan_out_drop_meets_the_stem() {
    let input = "graph TD
    subgraph Building [Building Access]
        Badge[Badge Reader]
        Door[Door Controller]
        Badge --> Door
    end
    subgraph Backend [Access Control Server]
        Check{Badge authorized?}
        Log[(Access Log)]
        Check --> Log
    end
    Door --> Check
    Check --> Unlock[Unlock Door]
    Check --> Deny[Deny and Alert]
    Deny --> Guard[Security Desk]
";
    for (style, pairs) in [
        (BaseStyle::Unicode, ["┴┬", "┬┴"]),
        (BaseStyle::Ascii, ["++", "++"]),
    ] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let bus = frame
            .lines()
            .skip_while(|line| !line.contains("Badge authorized?"))
            .find(|line| line.contains('┌') || line.contains("+-"))
            .expect("fan-out bus row");
        assert!(
            pairs.iter().all(|pair| !bus.contains(pair)),
            "stem and drop sit side by side on the bus ({style:?}):\n{frame}"
        );
    }
}

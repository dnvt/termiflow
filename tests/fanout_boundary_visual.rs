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

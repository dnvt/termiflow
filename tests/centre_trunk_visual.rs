use termiflow::{BaseStyle, RenderOptions};

const FAN_OUT: &str = "flowchart TB
    Accounts[Create Accounts] --> Email[Email]
    Accounts --> Slack[Slack]
    Accounts --> GitHub[GitHub]
";

const FAN_IN: &str = "graph TD
    Notify[Send Notification] --> Pref{User preference}
    Pref --> Push[Push]
    Pref --> SMS[SMS]
    Pref --> Email[Email]
    Push --> Delivered{Delivered?}
    SMS --> Delivered
    Email --> Delivered
";

fn render(input: &str, style: BaseStyle, optimized: bool) -> String {
    termiflow::render_with_feedback(
        input,
        RenderOptions::new()
            .with_style(style)
            .with_optimize_render(optimized),
    )
    .expect("render")
    .output
}

#[test]
fn even_width_middle_box_meets_the_trunk_on_its_other_centre_column() {
    for (name, input) in [("fan-out", FAN_OUT), ("fan-in", FAN_IN)] {
        for optimized in [false, true] {
            let frame = render(input, BaseStyle::Unicode, optimized);
            assert!(
                !frame.contains("┬┴") && !frame.contains("┴┬"),
                "{name}: the trunk must not land one column beside the middle branch \
                 (optimized={optimized}):\n{frame}"
            );
            assert!(
                frame.contains('┼'),
                "{name}: the trunk must run straight through the bus (optimized={optimized}):\n{frame}"
            );

            let frame = render(input, BaseStyle::Ascii, optimized);
            assert!(
                !frame.contains("++"),
                "{name}: ascii trunk must not land beside the middle branch \
                 (optimized={optimized}):\n{frame}"
            );
        }
    }
}

#[test]
fn even_width_fan_in_source_marks_the_stem_it_leaves_from() {
    for optimized in [false, true] {
        let frame = render(FAN_IN, BaseStyle::Unicode, optimized);
        let push_border = frame
            .lines()
            .skip_while(|line| !line.contains("Push"))
            .nth(1)
            .expect("Push bottom border");
        assert_eq!(
            push_border.matches('┬').count(),
            3,
            "each source border carries exactly one exit junction, none doubled \
             (optimized={optimized}):\n{frame}"
        );
        assert!(
            !push_border.contains("┬┬"),
            "doubled exit junction:\n{frame}"
        );
    }
}

#[test]
fn a_box_never_splits_its_entry_and_exit_across_its_two_centre_columns() {
    let input = "flowchart TB
    Accounts[Create Accounts] --> Email[Email]
    Accounts --> Slack[Slack]
    Accounts --> GitHub[GitHub]
    Email & Slack & GitHub --> Day1[Day One Orientation]
";
    for optimized in [false, true] {
        let frame = render(input, BaseStyle::Unicode, optimized);
        let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
        let label_row = rows
            .iter()
            .position(|row| row.iter().collect::<String>().contains("GitHub"))
            .expect("GitHub row");
        let text: Vec<char> = "GitHub".chars().collect();
        let text_col = rows[label_row]
            .windows(text.len())
            .position(|window| window == text.as_slice())
            .expect("GitHub column");
        let span = text_col.saturating_sub(3)..text_col + text.len() + 3;
        let entry = span
            .clone()
            .find(|&x| rows[label_row - 2].get(x) == Some(&'↓'))
            .unwrap_or_else(|| panic!("GitHub entry arrow:\n{frame}"));
        let exit = span
            .clone()
            .find(|&x| rows[label_row + 1].get(x) == Some(&'┬'))
            .unwrap_or_else(|| panic!("GitHub exit junction:\n{frame}"));
        assert_eq!(
            entry, exit,
            "GitHub's entry and exit must share one column (optimized={optimized}):\n{frame}"
        );
    }
}

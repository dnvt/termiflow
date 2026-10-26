use termiflow::{BaseStyle, RenderOptions};

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
fn an_odd_fan_out_runs_its_trunk_straight_into_the_middle_branch() {
    let input = "flowchart TB
    Triage[Triage Nurse] --> Acuity{Acuity level}
    Acuity -->|critical| Resus[Resuscitation Bay]
    Acuity -->|urgent| ED[ED Bed]
    Acuity -->|minor| FastTrack[Fast Track]
    Resus --> ICU[ICU]
";
    // The default render; the render optimizer re-lays out from its own
    // candidate positions, which this layout pass leaves alone.
    let frame = render(input, BaseStyle::Unicode, false);
    assert!(
        frame.contains('┼') && !frame.contains("┬┴") && !frame.contains("┴┬"),
        "the trunk must meet the middle branch on one column:\n{frame}"
    );
}

#[test]
fn a_fan_in_from_several_ranks_keeps_its_lanes_apart() {
    // Sources on different ranks are not one bus; lining up the middle one
    // would stack their long edges onto a single column.
    let input = "graph BT
    Customer((Customer)) --> Ticket[Open Ticket]
    Ticket --> L1[Tier 1 Support]
    L1 --> Solved1{Solved?}
    Solved1 -->|yes| Close[Close Ticket]
    Solved1 -->|no| L2[Tier 2 Support]
    L2 --> Solved2{Solved?}
    Solved2 -->|yes| Close
    Solved2 -->|no| Eng[Engineering Escalation]
    Eng --> Close
    Close --> CSAT[/CSAT Survey/]
";
    let plain = render(input, BaseStyle::Unicode, false);
    let below_tier_two = plain
        .lines()
        .skip_while(|line| !line.contains("Tier 2 Support"))
        .nth(2)
        .expect("row below Tier 2 Support");
    let lanes = below_tier_two
        .chars()
        .filter(|c| matches!(c, '│' | '↑'))
        .count();
    assert!(
        lanes >= 2,
        "the long fan-in legs must keep separate columns:\n{plain}"
    );
}

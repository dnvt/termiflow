use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

/// The border row next to the box labelled `label`, below it in TD and
/// above it in BT, cut to the box's columns.
fn rim(frame: &str, label: &str, below: bool) -> String {
    let lines: Vec<&str> = frame.lines().collect();
    let row = lines
        .iter()
        .position(|line| line.contains(label))
        .unwrap_or_else(|| panic!("no {label}:\n{frame}"));
    let start = lines[row][..lines[row].find(label).expect("label")]
        .chars()
        .count();
    let border = if below {
        lines[row + 1]
    } else {
        lines[row - 1]
    };
    border
        .chars()
        .skip(start)
        .take(label.chars().count())
        .collect()
}

/// gen_a_071: Service B's fan-in line to the registry ran straight down
/// through Service C, whose bottom then showed two exits. It now takes a
/// clear column beside Service C.
#[test]
fn a_td_fan_in_line_steps_around_a_box_in_its_column() {
    let input = "graph TD
    A[Service A] <--> B[Service B]
    B <--> C[Service C]
    A --o Reg[(Service Registry)]
    B --o Reg
    C --o Reg
    Reg --> Health[Health Checker]
    Health --x Dead[Deregister Dead]
";
    let frame = render(input);
    assert_eq!(
        rim(&frame, "  Service C  ", true).matches('┬').count(),
        1,
        "a line runs through Service C:\n{frame}"
    );
}

/// gen_a_071 flipped to BT: Service B's fan-in line to the registry ran
/// straight up through Service C. It now takes a clear column beside it.
#[test]
fn a_bt_fan_in_line_steps_around_a_box_in_its_column() {
    let input = "graph BT
    A[Service A] <--> B[Service B]
    B <--> C[Service C]
    A --o Reg[(Service Registry)]
    B --o Reg
    C --o Reg
    Reg --> Health[Health Checker]
    Health --x Dead[Deregister Dead]
";
    let frame = render(input);
    let row = frame
        .lines()
        .find(|line| line.contains("  Service C  "))
        .expect("Service C");
    let before = &row[..row.find("  Service C  ").expect("label")];
    assert_eq!(
        before.matches('│').count(),
        2,
        "no lane beside Service C:\n{frame}"
    );
}

/// gen_a_091: Helpful? -> Close Chat ("yes") skips a rank, and the box on
/// that rank, Hand off to Agent, stood in the decision's column: the line
/// ran hidden through it. The box now stands beside the column.
#[test]
fn a_skip_edge_drops_beside_the_box_on_the_rank_it_skips() {
    let input = "graph TD
    User((User)) --> Bot[Support Chatbot]
    Bot --> Intent{Understood?}
    Intent -->|yes| FAQ[Answer from KB]
    Intent -->|no| Rephrase[Ask to Rephrase]
    Rephrase --> Intent
    FAQ --> Helpful{Helpful?}
    Helpful -->|yes| Done([Close Chat])
    Helpful -->|no| Human[Hand off to Agent]
    Human --> Done
";
    let frame = render(input);
    let lines: Vec<&str> = frame.lines().collect();
    let row = lines
        .iter()
        .position(|line| line.contains("Hand off to Agent"))
        .expect("Hand off to Agent");
    let helpful = lines
        .iter()
        .find(|line| line.contains("Helpful?"))
        .expect("Helpful?");
    let column = helpful[..helpful.find("Helpful?").expect("label")]
        .chars()
        .count()
        + 4;
    let below: Vec<char> = lines[row].chars().collect();
    assert_eq!(
        below.get(column),
        Some(&'│'),
        "the decision's line is not beside Hand off to Agent:\n{frame}"
    );
}

/// The index of the first row holding `text`.
fn row_of(frame: &str, text: &str) -> usize {
    frame
        .lines()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no {text}:\n{frame}"))
}

/// gen_a_045: Dec -> Block ("yes") shares its stem with the branch to Manual
/// Review, and "yes" was drawn on that branch's turn. It now sits on its own
/// run toward Block, below Manual Review.
#[test]
fn a_td_branch_label_stays_on_its_own_branch() {
    let input = "flowchart TB
    Txn[Card Transaction] --> Rules[Rules Engine]
    Txn --> Model[Fraud Model]
    Rules --> Score[Risk Score]
    Model --> Score
    Score --> Dec{Score above 0.8?}
    Dec -->|yes| Block[Block and Notify]
    Dec -->|borderline| Review[Manual Review]
    Dec -->|no| Allow[Approve]
    Review --> Allow
    Review --> Block
";
    let frame = render(input);
    assert!(
        row_of(&frame, "yes") > row_of(&frame, "Manual Review"),
        "yes is on the branch to Manual Review:\n{frame}"
    );
}

/// gen_a_073: Underwriting -> Adverse Action ("decline") passes the tee where
/// the branch to Manual Underwriter leaves, and "decline" was drawn on that
/// branch's turn. It now sits on its own run, above Manual Underwriter.
#[test]
fn a_bt_branch_label_stays_on_its_own_branch() {
    let input = "graph BT
    Apply[Loan Application] --> KYC[KYC Check]
    KYC --> Credit[Credit Bureau Pull]
    Credit --> Underwrite{Underwriting}
    Underwrite -->|approve| Terms[Generate Terms]
    Underwrite -->|refer| Manual[Manual Underwriter]
    Underwrite -->|decline| Adverse[Adverse Action Notice]
    Manual --> Terms
    Manual --> Adverse
    Terms --> Fund[(Disburse Funds)]
";
    let frame = render(input);
    assert!(
        row_of(&frame, "decline") < row_of(&frame, "Manual Underwriter"),
        "decline is on the branch to Manual Underwriter:\n{frame}"
    );
}

/// gen_a_011 (TD) and gen_a_043 (BT): the run toward the target is too short
/// for the label, which was drawn over the other branch's turn. It now sits
/// beside its own stem, off every horizontal run.
#[test]
fn a_branch_label_without_room_on_its_run_sits_beside_its_stem() {
    let td = "graph TD
    Cart[Checkout Cart] --> Pay{Payment Method}
    Pay -->|card| Stripe[Stripe Charge]
    Pay -->|wallet| PayPal[PayPal Redirect]
    Stripe --> Auth3DS{3DS Required?}
    Auth3DS -->|yes| Challenge[3DS Challenge]
    Auth3DS -->|no| Capture[Capture Funds]
    Challenge --> Capture
    PayPal --> Capture
    Capture --> Receipt[/Send Receipt/]
    Capture --> Ledger[(Ledger)]
";
    let bt = "graph BT
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
    for (input, label) in [(td, "no"), (bt, "yes")] {
        let frame = render(input);
        let on_a_run = frame
            .lines()
            .filter(|line| line.contains(label))
            .any(|line| line.contains('─'));
        assert!(!on_a_run, "{label} is on another branch's run:\n{frame}");
    }
}

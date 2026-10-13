use termiflow::{BaseStyle, RenderOptions};

const TD_EXTERNAL_SOURCE_BESIDE_ISOLATED_MEMBER: &str = "graph TD
    GW[API Gateway]
    subgraph Services [Core Services]
        Auth[Auth Service]
        Orders[Order Service]
        Catalog[Catalog Service]
    end
    GW --> Auth
    GW --> Orders
";

#[test]
fn external_source_stays_outside_a_subgraph_whose_isolated_member_shares_its_rank() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                TD_EXTERNAL_SOURCE_BESIDE_ISOLATED_MEMBER,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let row_of = |needle: &str| {
                frame
                    .lines()
                    .position(|line| line.contains(needle))
                    .unwrap_or_else(|| panic!("{needle} not found:\n{frame}"))
            };
            assert!(
                row_of("API Gateway") < row_of("Core Services"),
                "API Gateway is not a member of Core Services and must sit above its frame \
                 ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

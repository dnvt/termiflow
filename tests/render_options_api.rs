fn explicit_nested_service_data_input(direction: &str) -> String {
    format!(
        "graph {direction}\nAPI[API Gateway]\nsubgraph SG1 [Service Layer]\nS1[User Service]\nS2[Order Service]\nsubgraph SG2 [Data Layer]\nD1[(User DB)]\nD2[(Order DB)]\nend\nResponse[Response Builder]\nS1 --> S2\nS1 --> D1\nS2 --> D2\nD1 --> Response\nD2 --> Response\nend\nAPI --> S1\n"
    )
}

fn rectangles_overlap(a: &termiflow::graph::Rectangle, b: &termiflow::graph::Rectangle) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

#[path = "render_options_api/feedback.rs"]
mod feedback;

#[path = "render_options_api/nested_subgraphs.rs"]
mod nested_subgraphs;

#[path = "render_options_api/direction_matrix.rs"]
mod direction_matrix;

use termiflow::{render_json, RenderOptions};

fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

fn rand_range(state: &mut u64, upper: usize) -> usize {
    if upper == 0 {
        return 0;
    }
    (lcg_next(state) as usize) % upper
}

fn rand_bool(state: &mut u64, numerator: usize, denominator: usize) -> bool {
    rand_range(state, denominator) < numerator
}

#[test]
fn fuzz_smoke_random_json_graphs_do_not_panic() {
    for seed in 0_u64..80 {
        let mut state = 0x9E3779B97F4A7C15_u64 ^ (seed.wrapping_mul(0xBF58476D1CE4E5B9));

        let node_count = 2 + rand_range(&mut state, 10);
        let edge_count = 1 + rand_range(&mut state, node_count.saturating_mul(2));

        let mut json = String::from("{\"direction\":\"TD\",\"nodes\":[");
        for i in 0..node_count {
            if i > 0 {
                json.push(',');
            }

            let label_variant = rand_range(&mut state, 5);
            let label = match label_variant {
                0 => format!("node-{i}"),
                1 => format!("node_{i}_a_very_long_identifier_with_delimiters"),
                2 => format!("node {i} with spaces and words"),
                3 => format!("node-{i}\\nmanual\\nbreaks"),
                _ => format!("node-{i}::module::path"),
            };

            let shape = match rand_range(&mut state, 6) {
                0 => "rectangle",
                1 => "rounded",
                2 => "stadium",
                3 => "diamond",
                4 => "circle",
                _ => "hexagon",
            };

            json.push_str(&format!(
                "{{\"id\":\"n{i}\",\"label\":{},\"shape\":\"{shape}\"}}",
                serde_json::to_string(&label).unwrap()
            ));
        }

        json.push_str("],\"edges\":[");
        for e in 0..edge_count {
            if e > 0 {
                json.push(',');
            }
            let from = rand_range(&mut state, node_count.saturating_sub(1));
            let to = from + 1 + rand_range(&mut state, node_count - from - 1);

            if rand_bool(&mut state, 1, 4) {
                json.push_str(&format!(
                    "{{\"from\":\"n{from}\",\"to\":\"n{to}\",\"label\":\"edge\"}}"
                ));
            } else {
                json.push_str(&format!("{{\"from\":\"n{from}\",\"to\":\"n{to}\"}}"));
            }
        }

        if rand_bool(&mut state, 2, 3) && node_count >= 3 {
            let start = rand_range(&mut state, node_count.saturating_sub(2));
            let end = start + 1 + rand_range(&mut state, (node_count - start).min(6));
            json.push_str("],\"subgraphs\":[");
            json.push_str("{\"id\":\"g0\",\"title\":\"Group\",\"nodes\":[");
            for i in start..end {
                if i > start {
                    json.push(',');
                }
                json.push_str(&format!("\"n{i}\""));
            }
            json.push_str("]}");
            json.push(']');
        } else {
            json.push(']');
        }

        json.push('}');

        for compact in [false, true] {
            let options = RenderOptions::default()
                .with_wrap_labels(true)
                .with_max_label_lines(3)
                .with_compact(compact);

            let output = render_json(&json, options).unwrap();

            assert!(
                !output.trim().is_empty(),
                "empty output for seed {seed} (compact={compact}); json={json}"
            );
            assert!(
                output.len() < 200_000,
                "output too large for seed {seed} (compact={compact})"
            );
        }
    }
}

//! Contract probes for configuration precedence and process-bound compatibility inputs.

use std::fs;

use termiflow::ScalingMode;

#[test]
fn environment_compatibility_is_captured_at_the_render_boundary() {
    let evidence_path = std::env::temp_dir().join(format!(
        "termiflow-configuration-contract-{}.json",
        std::process::id()
    ));
    let evidence_path_text = evidence_path.to_str().expect("temporary path is UTF-8");

    let mut command = assert_cmd::cargo::cargo_bin_cmd!("termiflow");
    command
        .env(
            "HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-home-{}",
                std::process::id()
            ),
        )
        .env(
            "XDG_CONFIG_HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-xdg-{}",
                std::process::id()
            ),
        )
        .env_remove("TERMIFLOW_OPTIMIZE_RENDER")
        .env_remove("TERMIFLOW_RENDER_REPAIR_PASSES")
        .env_remove("TERMIFLOW_LAYOUT_REPAIR_PASSES")
        .env_remove("TERMIFLOW_DEBUG_CRITIC")
        .args(["--audit-json", evidence_path_text])
        .env("TERMIFLOW_OPTIMIZE_RENDER", "1")
        .env("TERMIFLOW_RENDER_REPAIR_PASSES", "0")
        .write_stdin("flowchart TD\nA[Start] --> B[End]\n")
        .assert()
        .success();

    let evidence: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&evidence_path).expect("evidence is written"))
            .expect("evidence is valid JSON");
    let _ = fs::remove_file(&evidence_path);

    assert_eq!(evidence["optimized"], true);
    assert_eq!(evidence["repair_passes"], 1);
}

#[test]
fn default_cli_scaling_matches_explicit_fixed_scaling() {
    assert_eq!(ScalingMode::default(), ScalingMode::Fixed);

    let input = "flowchart TD\nA[Start] --> B[Process] --> C[Finish]\n";

    let mut default_command = assert_cmd::cargo::cargo_bin_cmd!("termiflow");
    let default_output = default_command
        .env(
            "HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-home-{}",
                std::process::id()
            ),
        )
        .env(
            "XDG_CONFIG_HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-xdg-{}",
                std::process::id()
            ),
        )
        .env_remove("TERMIFLOW_OPTIMIZE_RENDER")
        .env_remove("TERMIFLOW_RENDER_REPAIR_PASSES")
        .arg("--style")
        .arg("ascii")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut fixed_command = assert_cmd::cargo::cargo_bin_cmd!("termiflow");
    let fixed_output = fixed_command
        .env(
            "HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-home-{}",
                std::process::id()
            ),
        )
        .env(
            "XDG_CONFIG_HOME",
            format!(
                "/private/tmp/termiflow-configuration-contract-xdg-{}",
                std::process::id()
            ),
        )
        .env_remove("TERMIFLOW_OPTIMIZE_RENDER")
        .env_remove("TERMIFLOW_RENDER_REPAIR_PASSES")
        .args(["--style", "ascii", "--scaling", "fixed"])
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(default_output, fixed_output);
}

use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(arguments)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn help(command: &str) -> String {
    let output = run(&[command, "--help"]);
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn root_exposes_exactly_six_product_operations() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    let commands = help
        .lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|command| *command != "help")
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        ["validate", "select", "filter", "sort", "join", "groupby"]
    );
}

#[test]
fn nested_help_has_stable_sections_and_shared_output_contracts() {
    for command in ["validate", "select", "filter", "sort", "join", "groupby"] {
        let help = help(command);
        assert!(help.contains("Usage:"), "{command}: {help}");
        assert!(help.contains("Options:"), "{command}: {help}");
        assert!(!help.as_bytes().contains(&0x1b), "{command}: {help}");
    }
    for command in ["select", "filter", "sort", "join", "groupby"] {
        let help = help(command);
        assert_eq!(help.matches("--no-output-header").count(), 1, "{command}");
        assert!(help.contains("Omit the output header"), "{command}: {help}");
    }
}

#[test]
fn parse_errors_keep_usage_and_suggestions() {
    let output = run(&["groupby", "--agregate", "value:sum"]);
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("--aggregate"), "{error}");
    assert!(error.contains("Usage:"), "{error}");
}

#[test]
fn json_summary_and_table_data_use_separate_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let table = directory.path().join("output.csv");
    std::fs::write(&input, b"sample,value\ns1,1\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["--json", "select", "--fields", "value", "--output"])
        .arg(&table)
        .arg(&input)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["tool"], "rsomics-table");
    assert_eq!(std::fs::read(table).unwrap(), b"value\n1\n");
}

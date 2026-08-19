use std::io::{Read, Write};
use std::process::{Command, Output};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

const DATA: &[u8] = b"sample\tgroup\texpr\tdepth\n\
s1\tctrl\t1.5\t10\n\
s2\tctrl\t2.0\t12\n\
s3\tctrl\t3.5\t9\n\
s4\tctrl\t2.5\t11\n\
s5\tcase\t10.0\t30\n\
s6\tcase\t12.0\t28\n\
s7\tcase\t8.0\t35\n\
s8\tcase\t9.5\t31\n\
s9\tcase\t11.0\t29\n";

fn run(input: &[u8], arguments: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.csv");
    std::fs::write(&input_path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("groupby")
        .args(arguments)
        .arg(input_path)
        .output()
        .unwrap()
}

#[test]
fn global_composite_groups_are_collision_free_and_sorted() {
    let output = run(
        b"a,b,value\nab,c,2\na,bc,1\na,bc,3\nab,c,4\n",
        &[
            "--group",
            "a,b",
            "--aggregate",
            "value:sum=total",
            "--aggregate",
            "value:mean",
        ],
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"a,b,total,mean(value)\na,bc,4,2\nab,c,6,3\n"
    );
}

#[test]
fn complete_aggregate_set_has_stable_values_and_labels() {
    let output = run(
        DATA,
        &[
            "--tsv",
            "-a",
            "expr:sum",
            "-a",
            "expr:min",
            "-a",
            "expr:max",
            "-a",
            "expr:absmin",
            "-a",
            "expr:absmax",
            "-a",
            "expr:range",
            "-a",
            "expr:mean",
            "-a",
            "expr:geomean",
            "-a",
            "expr:harmmean",
            "-a",
            "expr:pvar",
            "-a",
            "expr:svar",
            "-a",
            "expr:pstdev",
            "-a",
            "expr:sstdev",
            "-a",
            "depth:median",
            "-a",
            "depth:q1",
            "-a",
            "depth:q3",
            "-a",
            "depth:iqr",
            "-a",
            "depth:perc:90",
            "-a",
            "depth:mad",
            "-a",
            "depth:madraw",
            "-a",
            "group:count",
            "-a",
            "sample:first",
            "-a",
            "sample:last",
            "-a",
            "group:unique",
            "-a",
            "group:collapse",
            "-a",
            "group:countunique",
            "-a",
            "group:mode",
            "-a",
            "group:antimode",
        ],
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let rows = text.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].split('\t').collect::<Vec<_>>(),
        [
            "sum(expr)",
            "min(expr)",
            "max(expr)",
            "absmin(expr)",
            "absmax(expr)",
            "range(expr)",
            "mean(expr)",
            "geomean(expr)",
            "harmmean(expr)",
            "pvar(expr)",
            "svar(expr)",
            "pstdev(expr)",
            "sstdev(expr)",
            "median(depth)",
            "q1(depth)",
            "q3(depth)",
            "iqr(depth)",
            "perc:90(depth)",
            "mad(depth)",
            "madraw(depth)",
            "count(group)",
            "first(sample)",
            "last(sample)",
            "unique(group)",
            "collapse(group)",
            "countunique(group)",
            "mode(group)",
            "antimode(group)",
        ]
    );
    assert_eq!(
        rows[1].split('\t').collect::<Vec<_>>(),
        [
            "60",
            "1.5",
            "12",
            "1.5",
            "12",
            "10.5",
            "6.6666666666667",
            "5.1688122704862",
            "3.8185970636215",
            "16",
            "18",
            "4",
            "4.2426406871193",
            "28",
            "11",
            "30",
            "19",
            "31.8",
            "10.3782",
            "7",
            "9",
            "s1",
            "s9",
            "case,ctrl",
            "ctrl,ctrl,ctrl,ctrl,case,case,case,case,case",
            "2",
            "case",
            "ctrl",
        ]
    );
}

#[test]
fn higher_moments_match_the_declared_tolerance() {
    let output = run(
        DATA,
        &[
            "--tsv",
            "-a",
            "expr:pskew",
            "-a",
            "expr:sskew",
            "-a",
            "expr:pkurt",
            "-a",
            "expr:skurt",
        ],
    );
    assert!(output.status.success());
    let row = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap()
        .split('\t')
        .map(|value| value.parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    for (actual, expected) in row.iter().zip([
        -0.084056712962963,
        -0.10189212298348,
        -1.7232711226852,
        -2.1395640432099,
    ]) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

#[test]
fn consecutive_groups_keep_run_order_and_reject_reappearance() {
    let valid = run(
        b"key,value\nB,1\nB,2\nA,4\n",
        &["--consecutive", "--group", "key", "-a", "value:sum"],
    );
    assert!(valid.status.success());
    assert_eq!(valid.stdout, b"key,sum(value)\nB,3\nA,4\n");

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&input, b"key,value\nA,1\nB,2\nA,3\n").unwrap();
    std::fs::write(&output, b"old\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args([
            "groupby",
            "--consecutive",
            "--group",
            "key",
            "-a",
            "value:sum",
            "--output",
        ])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("reappears")
    );
    assert_eq!(std::fs::read(output).unwrap(), b"old\n");
}

#[test]
fn numeric_failures_have_record_line_field_and_operation_context() {
    let output = run(
        b"key,value\nA,1\nA,nope\n",
        &["--group", "key", "-a", "value:sum"],
    );
    assert!(!output.status.success());
    assert_eq!(output.stdout, b"key,sum(value)\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    for context in ["record 3", "line 3", "field 2", "sum"] {
        assert!(stderr.contains(context), "{stderr}");
    }
}

#[test]
fn ignored_non_numeric_cells_are_counted_once_per_field() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&input, b"key,value\nA,1\nA,nope\nA,3\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args([
            "--json",
            "groupby",
            "--group",
            "key",
            "-a",
            "value:sum",
            "-a",
            "value:mean",
            "--ignore-non-numeric",
            "--output",
        ])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        std::fs::read(output).unwrap(),
        b"key,sum(value),mean(value)\nA,4,2\n"
    );
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["result"]["ignored_numeric_values"], 1);
    assert_eq!(summary["result"]["groups"], 1);
    assert_eq!(summary["result"]["input_records"], 3);
}

#[test]
fn byte_text_aggregates_preserve_values_and_use_checked_csv_output() {
    let output = run(
        b"g,\xff\ng,a\ng,\xff\n",
        &[
            "--no-header",
            "--group",
            "1",
            "-a",
            "2:first",
            "-a",
            "2:last",
            "-a",
            "2:unique",
            "-a",
            "2:collapse",
            "-a",
            "2:mode",
            "-a",
            "2:antimode",
        ],
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"g,\xff,\xff,\"a,\xff\",\"\xff,a,\xff\",\xff,a\n"
    );
}

#[test]
fn headerless_global_grouping_and_custom_collapse_delimiter_are_explicit() {
    let output = run(
        b"B,2,x\nA,1,y\nB,4,z\n",
        &[
            "--no-header",
            "--group",
            "1",
            "-a",
            "2:sum",
            "-a",
            "3:collapse",
            "--collapse-delimiter",
            "|",
        ],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"A,1,y\nB,6,x|z\n");
}

#[test]
fn malformed_aggregate_specs_and_duplicate_output_headers_fail_before_output() {
    for aggregate in ["value", "value:bogus", "value:perc:101", "value:sum="] {
        let output = run(b"key,value\nA,1\n", &["-a", aggregate]);
        assert!(!output.status.success(), "{aggregate}");
        assert!(output.stdout.is_empty(), "{aggregate}");
    }

    let multiple_fields = run(b"a,b\n1,2\n", &["-a", "1-2:sum"]);
    assert!(!multiple_fields.status.success());
    assert!(multiple_fields.stdout.is_empty());

    let duplicate = run(
        b"key,value\nA,1\n",
        &["--group", "key", "-a", "value:sum=key"],
    );
    assert!(!duplicate.status.success());
    assert!(duplicate.stdout.is_empty());
}

#[test]
fn braced_field_names_keep_operation_and_alias_separators_unambiguous() {
    let output = run(
        b"a:b,c=d\n1,2\n3,4\n",
        &["-a", "${a:b}:sum=left", "-a", "${c=d}:sum=right"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"left,right\n4,6\n");
}

#[test]
fn non_finite_numbers_fail_and_header_only_input_emits_only_its_schema() {
    for value in ["NaN", "inf", "-inf"] {
        let input = format!("value\n{value}\n");
        let output = run(input.as_bytes(), &["-a", "value:sum"]);
        assert!(!output.status.success(), "{value}");
    }

    let empty = run(b"key,value\n", &["--group", "key", "-a", "value:sum"]);
    assert!(empty.status.success());
    assert_eq!(empty.stdout, b"key,sum(value)\n");
}

#[test]
fn gzip_input_output_json_and_alias_checks_share_table_contracts() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.bin");
    let output = directory.path().join("output.csv.gz");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"key,value\nA,1\nA,2\n").unwrap();
    std::fs::write(&input, encoder.finish().unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args([
            "--json",
            "groupby",
            "--group",
            "key",
            "-a",
            "value:sum",
            "--output",
        ])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(result.status.success());
    let mut decoded = Vec::new();
    GzDecoder::new(std::fs::File::open(output).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"key,sum(value)\nA,3\n");
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["result"]["input_compression"], "gzip");
    assert_eq!(summary["result"]["output_compression"], "gzip");

    let alias = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["groupby", "-a", "value:sum", "--output"])
        .arg(&input)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!alias.status.success());
}

#[cfg(unix)]
#[test]
fn broken_stdout_fails_nonzero() {
    use std::net::Shutdown;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    std::fs::write(&input, b"value\n1\n").unwrap();
    let (_reader, writer) = UnixStream::pair().unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    let writer: OwnedFd = writer.into();
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["groupby", "-a", "value:sum"])
        .arg(input)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("I/O error")
    );
}

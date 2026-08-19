use std::io::{Read, Write};
use std::process::{Command, Output};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};

fn run(input: &[u8], args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.csv");
    std::fs::write(&input_path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("filter")
        .args(args)
        .arg(input_path)
        .output()
        .unwrap()
}

#[test]
fn filters_with_typed_precedence() {
    let output = run(
        b"id,a,b,name\nr1,2,4,keep\nr2,6,1,skip\nr3,8,1,keep\n",
        &["--where", "$a + $b * 2 >= 10 && $name != 'skip'"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"id,a,b,name\nr1,2,4,keep\nr3,8,1,keep\n");
}

#[test]
fn filters_with_regex_membership_and_lengths() {
    let output = run(
        "sample name,status,label\nS01,case,沈伟\nS02,other,沈伟\nbad,case,沈伟\n".as_bytes(),
        &[
            "--where",
            "${sample name} =~ '^S[0-9]+$' && $status in ('case', 'control') && len($label) == 6 && ulen($label) == 4",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        "sample name,status,label\nS01,case,沈伟\n".as_bytes()
    );
}

#[test]
fn headerless_fields_and_numeric_string_policy_are_explicit() {
    let numeric = run(b"001,A\n2,B\n", &["--no-header", "--where", "$1 == 1"]);
    assert!(numeric.status.success());
    assert_eq!(numeric.stdout, b"001,A\n");

    let text = run(
        b"001,A\n2,B\n",
        &[
            "--no-header",
            "--numeric-as-string",
            "--where",
            "$1 == '001'",
        ],
    );
    assert!(text.status.success());
    assert_eq!(text.stdout, b"001,A\n");
}

#[test]
fn unary_null_modulo_and_short_circuit_are_typed() {
    let unary = run(
        b"value\n2\n3\n",
        &[
            "--where",
            "null == null && true && !false && -$value % 2 == 0",
        ],
    );
    assert!(unary.status.success());
    assert_eq!(unary.stdout, b"value\n2\n");

    let short_circuit = run(
        b"value\n0\n2\n",
        &["--where", "$value != 0 && 10 / $value > 1"],
    );
    assert!(short_circuit.status.success());
    assert_eq!(short_circuit.stdout, b"value\n2\n");
}

#[test]
fn comparisons_not_match_and_negative_lists_are_closed() {
    let comparisons = run(
        b"value,label\n1,skip\n2,keep\n3,other\n4,keep\n",
        &[
            "--where",
            "$value >= 2 && $value <= 3 && $value > 1 && $value < 4 && $value != 4 && $label !~ '^skip$'",
        ],
    );
    assert!(comparisons.status.success());
    assert_eq!(comparisons.stdout, b"value,label\n2,keep\n3,other\n");

    let membership = run(b"value\n-2\n0\n2\n", &["--where", "$value in (-2, 2)"]);
    assert!(membership.status.success());
    assert_eq!(membership.stdout, b"value\n-2\n2\n");

    let unlike = run(b"value\n2\n", &["--where", "$value < '10'"]);
    assert!(unlike.status.success());
    assert_eq!(unlike.stdout, b"value\n");
}

#[test]
fn grouping_exponents_or_and_string_escapes_are_parsed_once() {
    let output = run(
        b"value,name\n2,O'Reilly\n3,other\n",
        &[
            "--where",
            "false || (($value + 1e0) * 2 == 6 && $name == 'O\\'Reilly' && $name == \"O'Reilly\")",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"value,name\n2,O'Reilly\n");
}

#[test]
fn arithmetic_type_and_nonfinite_errors_fail_loud() {
    let text = run(b"value\nx\n", &["--where", "$value + 1 > 0"]);
    assert!(!text.status.success());
    assert!(
        String::from_utf8(text.stderr)
            .unwrap()
            .contains("numeric operands")
    );

    let overflow = run(b"value\n1\n", &["--where", "1e400 > $value"]);
    assert!(!overflow.status.success());
    assert!(
        String::from_utf8(overflow.stderr)
            .unwrap()
            .contains("not finite")
    );
}

#[test]
fn invalid_expressions_fail_before_table_output() {
    for expression in [
        "$value",
        "$value =~ $pattern",
        "$value =~ '['",
        "$value ** 2 > 1",
        "$value ? true : false",
        "$value ?? 0",
        "($value > 1",
        "$value in ()",
        "$value in ($pattern)",
        "$value > 1e",
    ] {
        let output = run(b"value,pattern\n2,x\n", &["--where", expression]);
        assert!(!output.status.success(), "{expression}");
        assert!(output.stdout.is_empty(), "{expression}");
    }
}

#[test]
fn text_is_validated_only_when_the_expression_reads_it() {
    let unused = run(b"raw,value\n\xff,1\n", &["--where", "$value == 1"]);
    assert!(
        unused.status.success(),
        "{}",
        String::from_utf8_lossy(&unused.stderr)
    );
    assert_eq!(unused.stdout, b"raw,value\n\xff,1\n");

    let consumed = run(b"raw,value\n\xff,1\n", &["--where", "$raw == 'x'"]);
    assert!(!consumed.status.success());
    let stderr = String::from_utf8(consumed.stderr).unwrap();
    assert!(stderr.contains("record 2"), "{stderr}");
    assert!(stderr.contains("field 1"), "{stderr}");
    assert!(stderr.contains("UTF-8"), "{stderr}");
}

#[test]
fn expression_depth_is_bounded_before_evaluation() {
    let expression = format!("{}true{}", "(".repeat(129), ")".repeat(129));
    let output = run(b"value\n1\n", &["--where", &expression]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("nesting")
    );
}

#[test]
fn explicit_comments_do_not_change_record_context() {
    let output = run(
        b"# source\nid,value\n1,9\n2,2\n",
        &["--comment", "#", "--where", "$value > 5"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"id,value\n1,9\n");
}

#[test]
fn late_evaluation_error_preserves_named_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&input, b"value\n2\n0\n").unwrap();
    std::fs::write(&output, b"old\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["filter", "--where", "10 / $value > 1", "--output"])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("division by zero")
    );
    assert_eq!(std::fs::read(output).unwrap(), b"old\n");
}

#[test]
fn output_cannot_alias_input() {
    let directory = tempfile::tempdir().unwrap();
    let table = directory.path().join("input.csv");
    std::fs::write(&table, b"value\n2\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["filter", "--where", "$value > 1", "--output"])
        .arg(&table)
        .arg(&table)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(table).unwrap(), b"value\n2\n");
}

#[test]
fn output_dialect_header_and_quoting_are_preserved() {
    let output = run(
        b"id,description,keep\n1,\"a,b\",yes\n2,no,no\n",
        &[
            "--where",
            "$keep == 'yes'",
            "--output-tsv",
            "--no-output-header",
        ],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"1\ta,b\tyes\n");
}

#[test]
fn gzip_input_output_and_json_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.bin");
    let output = directory.path().join("filtered.csv.gz");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"id,value\n1,9\n2,2\n").unwrap();
    std::fs::write(&input, encoder.finish().unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["--json", "filter", "--where", "$value > 5", "--output"])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["result"]["records"], 2);
    assert_eq!(summary["result"]["kept"], 1);
    assert_eq!(summary["result"]["fields"], 2);
    assert_eq!(summary["result"]["input_compression"], "gzip");
    assert_eq!(summary["result"]["output_compression"], "gzip");

    let mut decoded = Vec::new();
    GzDecoder::new(std::fs::File::open(output).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"id,value\n1,9\n");
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
    std::fs::write(&input, b"name,value\nA,1\n").unwrap();
    let (_reader, writer) = UnixStream::pair().unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    let writer: OwnedFd = writer.into();
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["filter", "--where", "$value > 0"])
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

use std::process::Command;

use flate2::{Compression, write::GzEncoder};
use std::io::Write;

fn run(input: &[u8], args: &[&str]) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.tsv");
    std::fs::write(&input_path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("validate")
        .args(args)
        .arg(input_path)
        .output()
        .unwrap()
}

#[test]
fn valid_tsv_reports_shape() {
    let output = run(b"name\tvalue\nA\t1\nB\t2\n", &["--tsv"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"valid: 2 records, 2 fields\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn ragged_tsv_fails_with_record_context() {
    let output = run(b"name\tvalue\nA\t1\nB\n", &["--tsv"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("record 3"), "{stderr}");
    assert!(stderr.contains("1 field"), "{stderr}");
    assert!(stderr.contains("expected 2"), "{stderr}");
}

#[test]
fn quoted_newline_and_crlf_are_one_record() {
    let output = run(b"name,value\r\nA,\"left\r\nright\"\r\n", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"valid: 1 record, 2 fields\n");
}

#[test]
fn comments_are_only_skipped_when_explicit() {
    let with_comment = run(
        b"# source\nname\tvalue\nA\t1\n",
        &["--tsv", "--comment", "#"],
    );
    assert!(with_comment.status.success());
    assert_eq!(with_comment.stdout, b"valid: 1 record, 2 fields\n");

    let without_comment = run(b"# source\nname\tvalue\nA\t1\n", &["--tsv"]);
    assert!(!without_comment.status.success());
}

#[test]
fn duplicate_and_empty_headers_fail() {
    for input in [b"name\tname\nA\t1\n".as_slice(), b"name\t\nA\t1\n"] {
        let output = run(input, &["--tsv"]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("header field")
        );
    }
}

#[test]
fn empty_headerless_input_is_valid() {
    let output = run(b"", &["--no-header"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"valid: 0 records, 0 fields\n");

    let header_mode = run(b"", &[]);
    assert!(!header_mode.status.success());
}

#[test]
fn utf8_validation_is_opt_in() {
    let input = b"name,value\nA,\xff\n";
    assert!(run(input, &[]).status.success());
    let strict = run(input, &["--utf8"]);
    assert!(!strict.status.success());
    assert!(String::from_utf8(strict.stderr).unwrap().contains("UTF-8"));
}

#[test]
fn gzip_is_detected_by_magic_and_fully_checked() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"name,value\nA,1\n").unwrap();
    let encoded = encoder.finish().unwrap();
    let output = run(&encoded, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"valid: 1 record, 2 fields\n");

    let mut truncated = encoded;
    truncated.truncate(truncated.len() - 4);
    let output = run(&truncated, &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("gzip"));
}

#[test]
fn json_report_is_separate_and_structured() {
    let output = run(b"name\tvalue\nA\t1\n", &["--tsv", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["result"]["records"], 1);
    assert_eq!(envelope["result"]["fields"], 2);
    assert_eq!(envelope["result"]["compression"], "plain");
    assert_eq!(envelope["result"]["delimiter"], "\t");
}

#[test]
fn json_invalid_report_collects_safe_errors() {
    let output = run(
        b"a\tb\n1\n2\t3\t4\n",
        &["--tsv", "--json", "--max-errors", "2"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["report"]["errors"].as_array().unwrap().len(), 2);
}

#[test]
fn malformed_quote_fails_with_position() {
    let output = run(b"a,b\n1,a\"b\n", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("record 2"), "{stderr}");
    assert!(stderr.contains("bare quote"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn broken_stdout_fails_nonzero() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.csv");
    std::fs::write(&input_path, b"name,value\nA,1\n").unwrap();
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let writer: OwnedFd = writer.into();
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("validate")
        .arg(input_path)
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

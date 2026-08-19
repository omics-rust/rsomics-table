use std::process::{Command, Output};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use std::io::{Read, Write};

fn run(input: &[u8], args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.table");
    std::fs::write(&input_path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("select")
        .args(args)
        .arg(input_path)
        .output()
        .unwrap()
}

#[test]
fn selects_reorders_and_repeats_named_fields() {
    let output = run(
        b"id,name,score\n1,A,9\n2,B,8\n",
        &["--fields", "score,id,score"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"score,id,score\n9,1,9\n8,2,8\n");
}

#[test]
fn selects_ranges_and_exclusions() {
    let range = run(b"a\tb\tc\td\n1\t2\t3\t4\n", &["--tsv", "--fields", "2-,1"]);
    assert!(range.status.success());
    assert_eq!(range.stdout, b"b\tc\td\ta\n2\t3\t4\t1\n");

    let exclusion = run(b"a\tb\tc\td\n1\t2\t3\t4\n", &["--tsv", "--fields", "-2--3"]);
    assert!(exclusion.status.success());
    assert_eq!(exclusion.stdout, b"a\td\n1\t4\n");
}

#[test]
fn headerless_selection_is_index_only() {
    let output = run(
        b"1\tA\t9\n2\tB\t8\n",
        &["--tsv", "--no-header", "--fields", "3,1"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"9\t1\n8\t2\n");

    let named = run(b"1\tA\t9\n", &["--tsv", "--no-header", "--fields", "score"]);
    assert!(!named.status.success());
    assert!(String::from_utf8(named.stderr).unwrap().contains("header"));
}

#[test]
fn missing_fields_and_duplicate_headers_fail_loud() {
    let missing = run(b"id,name\n1,A\n", &["--fields", "score"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8(missing.stderr).unwrap().contains("score"));

    let duplicate = run(b"id,id\n1,2\n", &["--fields", "1"]);
    assert!(!duplicate.status.success());
    assert!(
        String::from_utf8(duplicate.stderr)
            .unwrap()
            .contains("duplicated")
    );
}

#[test]
fn braced_and_fuzzy_names_share_one_plan() {
    let output = run(
        b"id,\"sample,name\",scoreA,scoreB\n1,S1,9,8\n",
        &["--fuzzy-fields", "--fields", "${sample,name},score*"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\"sample,name\",scoreA,scoreB\nS1,9,8\n");
}

#[test]
fn projected_records_keep_csv_quoting_and_bytes() {
    let quoted = run(
        b"id,description\n1,\"a,b\"\n2,\"left\nright\"\n",
        &["--fields", "description"],
    );
    assert!(quoted.status.success());
    assert_eq!(quoted.stdout, b"description\n\"a,b\"\n\"left\nright\"\n");

    let bytes = run(b"a,b\n1,\xff\n", &["--fields", "2"]);
    assert!(bytes.status.success());
    assert_eq!(bytes.stdout, b"b\n\xff\n");

    let invalid_unselected_header = run(b"name,\xff\nA,x\n", &["--fields", "name"]);
    assert!(
        invalid_unselected_header.status.success(),
        "{}",
        String::from_utf8_lossy(&invalid_unselected_header.stderr)
    );
    assert_eq!(invalid_unselected_header.stdout, b"name\nA\n");
}

#[test]
fn output_dialect_and_header_are_explicit() {
    let output = run(
        b"id,description\n1,\"a,b\"\n",
        &[
            "--fields",
            "description,id",
            "--output-tsv",
            "--no-output-header",
        ],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"a,b\t1\n");
}

#[test]
fn named_output_is_transactional_and_cannot_alias_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&input, b"a,b\n1,2\n3\n").unwrap();
    std::fs::write(&output, b"old\n").unwrap();

    let failed = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["select", "--fields", "1", "--output"])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(std::fs::read(&output).unwrap(), b"old\n");

    let alias = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["select", "--fields", "1", "--output"])
        .arg(&input)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!alias.status.success());
    assert_eq!(std::fs::read(&input).unwrap(), b"a,b\n1,2\n3\n");
}

#[test]
fn gzip_input_output_and_json_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.bin");
    let output = directory.path().join("selected.csv.gz");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"id,name\n1,A\n2,B\n").unwrap();
    std::fs::write(&input, encoder.finish().unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["--json", "select", "--fields", "name", "--output"])
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
    assert_eq!(summary["result"]["fields"], 1);
    assert_eq!(summary["result"]["input_compression"], "gzip");
    assert_eq!(summary["result"]["output_compression"], "gzip");

    let mut decoded = Vec::new();
    GzDecoder::new(std::fs::File::open(output).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"name\nA\nB\n");

    let conflicting = run(b"id,name\n1,A\n", &["--json", "--fields", "name"]);
    assert!(!conflicting.status.success());
    assert!(conflicting.stdout.is_empty());
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
        .args(["select", "--fields", "value"])
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

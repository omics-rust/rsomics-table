use std::io::{Read, Write};
use std::process::{Command, Output};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};

fn run(left: &[u8], right: &[u8], args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let left_path = directory.path().join("left.csv");
    let right_path = directory.path().join("right.csv");
    std::fs::write(&left_path, left).unwrap();
    std::fs::write(&right_path, right).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("join")
        .args(args)
        .arg(left_path)
        .arg(right_path)
        .output()
        .unwrap()
}

#[test]
fn duplicate_keys_produce_the_complete_cartesian_product() {
    let output = run(
        b"key,left\na,L1\na,L2\nb,L3\n",
        b"key,right\na,R1\na,R2\nc,R3\n",
        &["--on", "key"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"key,left,right\na,L1,R1\na,L1,R2\na,L2,R1\na,L2,R2\n"
    );
}

#[test]
fn left_and_full_joins_fill_unmatched_rows_deterministically() {
    let left = b"key,left\na,L1\nb,L2\n";
    let right = b"key,right\na,R1\nc,R3\nd,R4\n";
    let left_join = run(
        left,
        right,
        &["--on", "key", "--kind", "left", "--fill", "NA"],
    );
    assert!(left_join.status.success());
    assert_eq!(left_join.stdout, b"key,left,right\na,L1,R1\nb,L2,NA\n");

    let full = run(
        left,
        right,
        &["--on", "key", "--kind", "full", "--fill", "NA"],
    );
    assert!(full.status.success());
    assert_eq!(
        full.stdout,
        b"key,left,right\na,L1,R1\nb,L2,NA\nc,NA,R3\nd,NA,R4\n"
    );
}

#[test]
fn composite_keys_are_collision_free() {
    let output = run(
        b"a,b,left\nx,y,L0\na,bc,L1\nab,c,L2\n",
        b"a,b,right\nab,c,R2\na,bc,R1\nx,y,R0\n",
        &["--on", "a,b"],
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"a,b,left,right\nx,y,L0,R0\na,bc,L1,R1\nab,c,L2,R2\n"
    );
}

#[test]
fn differently_named_keys_and_duplicate_columns_build_a_checked_schema() {
    let output = run(
        b"id,value,shared\nA,L1,x\n",
        b"sample_id,value,shared\nA,R1,y\n",
        &[
            "--left-on",
            "id",
            "--right-on",
            "sample_id",
            "--right-suffix",
            "_r",
        ],
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"id,value,shared,value_r,shared_r\nA,L1,x,R1,y\n"
    );

    let collision = run(
        b"id,value,value_right\nA,L1,x\n",
        b"id,value\nA,R1\n",
        &["--on", "id"],
    );
    assert!(!collision.status.success());
    assert!(collision.stdout.is_empty());
    assert!(
        String::from_utf8(collision.stderr)
            .unwrap()
            .contains("value_right")
    );

    let full = run(
        b"id,left\nA,L1\n",
        b"sample_id,right\nB,R2\n",
        &[
            "--left-on",
            "id",
            "--right-on",
            "sample_id",
            "--kind",
            "full",
            "--fill",
            "NA",
        ],
    );
    assert!(full.status.success());
    assert_eq!(full.stdout, b"id,left,right\nA,L1,NA\nB,NA,R2\n");
}

#[test]
fn headerless_join_uses_indices_and_omits_right_keys() {
    let output = run(
        b"A,L1\nB,L2\n",
        b"R1,A\nR2,C\n",
        &[
            "--no-header",
            "--left-on",
            "1",
            "--right-on",
            "2",
            "--kind",
            "left",
            "--fill",
            "NA",
        ],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"A,L1,R1\nB,L2,NA\n");
}

#[test]
fn case_folding_and_null_matching_are_explicit() {
    let left = b"key,left\nA,L1\n,L0\n";
    let right = b"key,right\na,R1\n,R0\n";
    let default = run(left, right, &["--on", "key", "--ignore-case"]);
    assert!(default.status.success());
    assert_eq!(default.stdout, b"key,left,right\nA,L1,R1\n,L0,R0\n");

    let no_null = run(
        left,
        right,
        &["--on", "key", "--ignore-case", "--null-never-matches"],
    );
    assert!(no_null.status.success());
    assert_eq!(no_null.stdout, b"key,left,right\nA,L1,R1\n");
}

#[test]
fn key_arity_and_consumed_utf8_fail_loud() {
    let arity = run(
        b"a,b\n1,2\n",
        b"x,y\n1,2\n",
        &["--left-on", "a,b", "--right-on", "x"],
    );
    assert!(!arity.status.success());
    assert!(arity.stdout.is_empty());

    let repeated = run(b"a,b\n1,2\n", b"a,b\n1,2\n", &["--on", "a,a"]);
    assert!(!repeated.status.success());
    assert!(
        String::from_utf8(repeated.stderr)
            .unwrap()
            .contains("repeats field")
    );

    let utf8 = run(
        b"key,left\n\xff,L1\n",
        b"key,right\n\xff,R1\n",
        &["--on", "key", "--ignore-case"],
    );
    assert!(!utf8.status.success());
    let stderr = String::from_utf8(utf8.stderr).unwrap();
    assert!(stderr.contains("record 2"), "{stderr}");
    assert!(stderr.contains("field 1"), "{stderr}");
    assert!(stderr.contains("UTF-8"), "{stderr}");

    let preserved = run(
        b"key,left\nA,\xff\n",
        b"key,right\na,\xfe\n",
        &["--on", "key", "--ignore-case"],
    );
    assert!(preserved.status.success());
    assert_eq!(preserved.stdout, b"key,left,right\nA,\xff,\xfe\n");
}

#[test]
fn late_structural_error_preserves_named_output() {
    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left.csv");
    let right = directory.path().join("right.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&left, b"key,left\na,L1\nb\n").unwrap();
    std::fs::write(&right, b"key,right\na,R1\n").unwrap();
    std::fs::write(&output, b"old\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["join", "--on", "key", "--output"])
        .arg(&output)
        .arg(&left)
        .arg(&right)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(output).unwrap(), b"old\n");
}

#[test]
fn output_cannot_alias_either_input() {
    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left.csv");
    let right = directory.path().join("right.csv");
    std::fs::write(&left, b"key,left\na,L1\n").unwrap();
    std::fs::write(&right, b"key,right\na,R1\n").unwrap();
    for output in [&left, &right] {
        let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
            .args(["join", "--on", "key", "--output"])
            .arg(output)
            .arg(&left)
            .arg(&right)
            .output()
            .unwrap();
        assert!(!result.status.success());
    }
    assert_eq!(std::fs::read(left).unwrap(), b"key,left\na,L1\n");
    assert_eq!(std::fs::read(right).unwrap(), b"key,right\na,R1\n");
}

#[test]
fn incompatible_stream_configurations_fail_before_reading() {
    let both_stdin = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["join", "--on", "key", "-", "-"])
        .output()
        .unwrap();
    assert!(!both_stdin.status.success());
    assert!(
        String::from_utf8(both_stdin.stderr)
            .unwrap()
            .contains("only one join input")
    );

    let json_to_stdout = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["--json", "join", "--on", "key", "left.csv", "right.csv"])
        .output()
        .unwrap();
    assert!(!json_to_stdout.status.success());
    assert!(
        String::from_utf8(json_to_stdout.stderr)
            .unwrap()
            .contains("--json requires a named table output")
    );
}

#[test]
fn gzip_inputs_output_and_json_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left.bin");
    let right = directory.path().join("right.bin");
    let output = directory.path().join("joined.csv.gz");
    for (path, contents) in [
        (&left, b"key,left\na,L1\n".as_slice()),
        (&right, b"key,right\na,R1\n".as_slice()),
    ] {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(contents).unwrap();
        std::fs::write(path, encoder.finish().unwrap()).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["--json", "join", "--on", "key", "--output"])
        .arg(&output)
        .arg(&left)
        .arg(&right)
        .output()
        .unwrap();
    assert!(result.status.success());
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["result"]["left_records"], 1);
    assert_eq!(summary["result"]["right_records"], 1);
    assert_eq!(summary["result"]["output_records"], 1);
    assert_eq!(summary["result"]["left_compression"], "gzip");
    assert_eq!(summary["result"]["right_compression"], "gzip");
    assert_eq!(summary["result"]["output_compression"], "gzip");

    let mut decoded = Vec::new();
    GzDecoder::new(std::fs::File::open(output).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"key,left,right\na,L1,R1\n");
}

#[cfg(unix)]
#[test]
fn broken_stdout_fails_nonzero() {
    use std::net::Shutdown;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left.csv");
    let right = directory.path().join("right.csv");
    std::fs::write(&left, b"key,left\na,L1\n").unwrap();
    std::fs::write(&right, b"key,right\na,R1\n").unwrap();
    let (_reader, writer) = UnixStream::pair().unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    let writer: OwnedFd = writer.into();
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["join", "--on", "key"])
        .arg(left)
        .arg(right)
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

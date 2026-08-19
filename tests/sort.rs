use std::io::{Read, Write};
use std::process::{Command, Output};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};

fn run(input: &[u8], args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("input.csv");
    std::fs::write(&input_path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("sort")
        .args(args)
        .arg(input_path)
        .output()
        .unwrap()
}

#[test]
fn sorts_repeated_keys_by_type_and_direction() {
    let output = run(
        b"id,group,score\n1,b,10\n2,a,2\n3,a,10\n4,b,2\n",
        &["--key", "score:nr", "--key", "group"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"id,group,score\n3,a,10\n1,b,10\n2,a,2\n4,b,2\n"
    );
}

#[test]
fn natural_case_folded_and_range_keys_are_supported() {
    let natural = run(
        "name,value\nChr10,2\nchr2,9\nchr1,3\n".as_bytes(),
        &["--key", "name:N", "--ignore-case"],
    );
    assert!(natural.status.success());
    assert_eq!(
        natural.stdout,
        "name,value\nchr1,3\nchr2,9\nChr10,2\n".as_bytes()
    );

    let range = run(b"id,a,b\n1,x,2\n2,x,1\n3,a,9\n", &["--key", "2-3"]);
    assert!(range.status.success());
    assert_eq!(range.stdout, b"id,a,b\n3,a,9\n2,x,1\n1,x,2\n");
}

#[test]
fn comma_lists_braced_names_and_exclusions_share_field_rules() {
    let comma = run(
        b"id,group,value\n2,b,1\n1,b,9\n3,a,5\n",
        &["--key", "group,value:n"],
    );
    assert!(comma.status.success());
    assert_eq!(comma.stdout, b"id,group,value\n3,a,5\n2,b,1\n1,b,9\n");

    let braced = run(b"id,group name\n1,z\n2,a\n", &["--key", "${group name}"]);
    assert!(braced.status.success());
    assert_eq!(braced.stdout, b"id,group name\n2,a\n1,z\n");

    let exclusion = run(b"id,a,b\n1,x,2\n2,x,1\n", &["--key", "-1"]);
    assert!(exclusion.status.success());
    assert_eq!(exclusion.stdout, b"id,a,b\n2,x,1\n1,x,2\n");
}

#[test]
fn default_all_fields_and_headerless_sort_are_explicit() {
    let default = run(b"a,b\ny,1\nx,9\nx,2\n", &[]);
    assert!(default.status.success());
    assert_eq!(default.stdout, b"a,b\nx,2\nx,9\ny,1\n");

    let headerless = run(
        b"10,z\n2,a\n",
        &["--no-header", "--key", "1:n", "--no-output-header"],
    );
    assert!(headerless.status.success());
    assert_eq!(headerless.stdout, b"2,a\n10,z\n");
}

#[test]
fn numeric_keys_match_csvtk_special_value_policy() {
    let output = run(
        b"id,value\ninf,Inf\nninf,-Inf\nnan,NaN\nover,1e400\ncomma,\"3,000\"\nnum,5\ntext,x\nempty,\n",
        &["--key", "value:n"],
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"id,value\nninf,-Inf\nnum,5\ncomma,\"3,000\"\ntext,x\nnan,NaN\nover,1e400\nempty,\ninf,Inf\n"
    );
}

#[test]
fn serial_and_parallel_tie_permutations_are_identical() {
    let mut input = String::from("id,group,value\n");
    for index in 0..20_000 {
        input.push_str(&format!(
            "{index},group_{},{}\n",
            index % 10,
            (index * 17) % 997
        ));
    }
    let serial = run(input.as_bytes(), &["--threads", "1", "--key", "group"]);
    let parallel = run(input.as_bytes(), &["--threads", "4", "--key", "group"]);
    assert!(serial.status.success());
    assert!(parallel.status.success());
    assert_eq!(serial.stdout, parallel.stdout);
}

#[test]
fn key_text_is_validated_without_rejecting_unselected_bytes() {
    let preserved = run(b"name,raw\nb,\xff\na,\xfe\n", &["--key", "name"]);
    assert!(preserved.status.success());
    assert_eq!(preserved.stdout, b"name,raw\na,\xfe\nb,\xff\n");

    let consumed = run(
        b"name,raw\nb,\xff\na,\xfe\n",
        &["--key", "raw", "--ignore-case"],
    );
    assert!(!consumed.status.success());
    let stderr = String::from_utf8(consumed.stderr).unwrap();
    assert!(stderr.contains("record 2"), "{stderr}");
    assert!(stderr.contains("field 2"), "{stderr}");
    assert!(stderr.contains("UTF-8"), "{stderr}");
}

#[test]
fn unsupported_key_types_and_missing_fields_fail_before_output() {
    for key in ["value:d", "value:u", "missing"] {
        let output = run(b"value\n2\n", &["--key", key]);
        assert!(!output.status.success(), "{key}");
        assert!(output.stdout.is_empty(), "{key}");
    }
}

#[test]
fn late_structural_error_preserves_named_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("output.csv");
    std::fs::write(&input, b"id,value\n1,9\n2\n").unwrap();
    std::fs::write(&output, b"old\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["sort", "--key", "value:n", "--output"])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(output).unwrap(), b"old\n");
}

#[test]
fn output_cannot_alias_input() {
    let directory = tempfile::tempdir().unwrap();
    let table = directory.path().join("input.csv");
    std::fs::write(&table, b"value\n2\n1\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["sort", "--key", "value:n", "--output"])
        .arg(&table)
        .arg(&table)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(table).unwrap(), b"value\n2\n1\n");
}

#[test]
fn gzip_input_output_and_json_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.bin");
    let output = directory.path().join("sorted.tsv.gz");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"id,value\n1,9\n2,2\n").unwrap();
    std::fs::write(&input, encoder.finish().unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args([
            "--json",
            "sort",
            "--key",
            "value:n",
            "--output-tsv",
            "--output",
        ])
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
    assert_eq!(summary["result"]["fields"], 2);
    assert_eq!(summary["result"]["keys"], 1);
    assert_eq!(summary["result"]["input_compression"], "gzip");
    assert_eq!(summary["result"]["output_compression"], "gzip");

    let mut decoded = Vec::new();
    GzDecoder::new(std::fs::File::open(output).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"id\tvalue\n2\t2\n1\t9\n");
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
        .args(["sort", "--key", "value:n"])
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

use std::process::{Command, Output};

fn run(input: &[u8], arguments: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.csv");
    std::fs::write(&path, input).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("crosstab")
        .args(arguments)
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn long_measurements_become_a_global_sorted_matrix() {
    let input = b"sample\tfeature\tvalue\nb\tx\t21\na\ty\t7\na\tx\t3\na\ty\t2\na\tx\t40\n";
    for (aggregate, expected) in [
        (None, b"\tx\ty\na\t2\t2\nb\t1\tN/A\n".as_slice()),
        (
            Some("value:sum"),
            b"\tx\ty\na\t43\t9\nb\t21\tN/A\n".as_slice(),
        ),
    ] {
        let mut arguments = vec!["--tsv", "--row", "sample", "--column", "feature"];
        if let Some(aggregate) = aggregate {
            arguments.extend(["--aggregate", aggregate]);
        }
        let output = run(input, &arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
    }
    let quoted = run(
        b"sample,feature,value\n\"a,b\",\"x,y\",3\nz,q,5\n\"a,b\",\"x,y\",4\n",
        &[
            "--row",
            "1",
            "--column",
            "2",
            "--aggregate",
            "3:sum",
            "--filler",
            "0",
        ],
    );
    assert!(
        quoted.status.success(),
        "{}",
        String::from_utf8_lossy(&quoted.stderr)
    );
    assert_eq!(quoted.stdout, b",q,\"x,y\"\n\"a,b\",0,7\nz,5,0\n");
    let byte_header = run(
        b"\xff,feature,value\na,x,3\na,x,4\n",
        &["--row", "1", "--column", "2"],
    );
    assert!(
        byte_header.status.success(),
        "{}",
        String::from_utf8_lossy(&byte_header.stderr)
    );
    assert_eq!(byte_header.stdout, b",x\na,2\n");
    let directory = tempfile::tempdir().unwrap();
    let matrix = directory.path().join("matrix.csv");
    std::fs::write(&matrix, &quoted.stdout).unwrap();
    let selected = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args(["select", "--no-header", "--fields", "1,3"])
        .arg(&matrix)
        .output()
        .unwrap();
    assert!(selected.status.success());
    assert_eq!(selected.stdout, b",\"x,y\"\n\"a,b\",7\nz,0\n");
    let compressed = run(
        input,
        &["--tsv", "--row", "sample", "--column", "feature", "--gzip"],
    );
    assert!(compressed.status.success());
    let mut decoded = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(compressed.stdout.as_slice()),
        &mut decoded,
    )
    .unwrap();
    assert_eq!(decoded, b"\tx\ty\na\t2\t2\nb\t1\tN/A\n");
}

#[test]
fn malformed_input_preserves_named_destination() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.csv");
    let output = directory.path().join("matrix.csv");
    std::fs::write(&input, b"sample,feature,value\na,x,3\nb,y,not-a-number\n").unwrap();
    std::fs::write(&output, b"prior result\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .args([
            "crosstab",
            "--row",
            "sample",
            "--column",
            "feature",
            "--aggregate",
            "value:sum",
        ])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("expected a finite number"));
    assert_eq!(std::fs::read(output).unwrap(), b"prior result\n");
}

#[test]
#[ignore = "requires pinned GNU datamash 1.9"]
fn live_datamash_count_and_sum() {
    use std::process::Stdio;

    let datamash = std::env::var("RSOMICS_DATAMASH").expect("GNU datamash 1.9 path required");
    let version = Command::new(&datamash).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8_lossy(&version.stdout)
            .lines()
            .next()
            .unwrap()
            .contains("GNU datamash) 1.9")
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.tsv");
    let input =
        b"sample\tfeature\tvalue\nb\tx\t21\na\ty\t7\na\tx\t3\na\ty\t2\na\tx\t40\n\tx\t5\na\t\t6\n";
    std::fs::write(&path, input).unwrap();
    for aggregate in [None, Some("value:sum")] {
        let mut arguments = vec!["--tsv", "--row", "sample", "--column", "feature"];
        let mut source_arguments = vec!["--sort", "--header-in", "crosstab", "sample,feature"];
        if let Some(aggregate) = aggregate {
            arguments.extend(["--aggregate", aggregate]);
            source_arguments.extend(["sum", "value"]);
        }
        let native = run(input, &arguments);
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        let source = Command::new(&datamash)
            .args(source_arguments)
            .env("LC_ALL", "C")
            .env("TMPDIR", directory.path())
            .stdin(Stdio::from(std::fs::File::open(&path).unwrap()))
            .output()
            .unwrap();
        assert!(
            source.status.success(),
            "{}",
            String::from_utf8_lossy(&source.stderr)
        );
        assert_eq!(native.stdout, source.stdout);
    }
}

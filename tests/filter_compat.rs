use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str, contents: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (directory, path)
}

fn assert_matches_csvtk(input: &Path, expression: &str, ours: &[&str], upstream: &[&str]) {
    let ours = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("filter")
        .args(ours)
        .args(["--where", expression])
        .arg(input)
        .output()
        .unwrap();
    let csvtk = std::env::var("RSOMICS_CSVTK").expect("RSOMICS_CSVTK must name csvtk 0.37.0");
    let version = Command::new(&csvtk).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        "csvtk v0.37.0"
    );
    let upstream = Command::new(csvtk)
        .arg("filter2")
        .args(upstream)
        .args(["--filter", expression])
        .arg(input)
        .output()
        .unwrap();
    assert_eq!(
        ours.status.success(),
        upstream.status.success(),
        "expression: {expression}\nours: {}\ncsvtk: {}",
        String::from_utf8_lossy(&ours.stderr),
        String::from_utf8_lossy(&upstream.stderr)
    );
    assert_eq!(ours.stdout, upstream.stdout, "expression: {expression}");
}

#[test]
#[ignore = "requires pinned csvtk 0.37.0"]
fn live_csvtk_filter2_differential() {
    let (_directory, csv) = fixture(
        "input.csv",
        "id,age,status,sample name,label\n1,20,control,S01,沈伟\n2,30,case,S02,other\n3,40,other,X03,沈伟\n"
            .as_bytes(),
    );
    for expression in [
        "$age >= 30 && $status != 'other'",
        "$id % 2 == 1",
        "$status in ('case', 'control')",
        "${sample name} =~ '^S[0-9]+$'",
        "len($label) == 6 && ulen($label) == 4",
    ] {
        assert_matches_csvtk(&csv, expression, &[], &[]);
    }

    let (_directory, headerless) = fixture("headerless.tsv", b"001\tA\n2\tB\n");
    assert_matches_csvtk(
        &headerless,
        "$1 == '001'",
        &["--tsv", "--no-header", "--numeric-as-string"],
        &["-tT", "--no-header-row", "--numeric-as-string"],
    );
}

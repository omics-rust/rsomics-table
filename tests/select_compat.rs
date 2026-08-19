use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str, contents: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (directory, path)
}

fn assert_matches_csvtk(input: &Path, ours: &[&str], upstream: &[&str]) {
    let ours = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("select")
        .args(ours)
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
        .arg("cut")
        .args(upstream)
        .arg(input)
        .output()
        .unwrap();
    assert_eq!(ours.status.success(), upstream.status.success());
    assert_eq!(ours.stdout, upstream.stdout);
}

#[test]
#[ignore = "requires pinned csvtk 0.37.0"]
fn live_csvtk_differential() {
    let (_directory, csv) = fixture("input.csv", b"id,name,score\n1,A,9\n2,B,8\n");
    assert_matches_csvtk(
        &csv,
        &["--fields", "score,id,score"],
        &["--fields", "score,id,score"],
    );

    let (_directory, tsv) = fixture("input.tsv", b"a\tb\tc\td\n1\t2\t3\t4\n");
    assert_matches_csvtk(
        &tsv,
        &["--tsv", "--fields", "2-,1"],
        &["-tT", "--fields", "2-,1"],
    );
    assert_matches_csvtk(
        &tsv,
        &["--tsv", "--fields", "-2--3"],
        &["-tT", "--fields", "-2--3"],
    );

    let (_directory, headerless) = fixture("headerless.tsv", b"1\tA\t9\n2\tB\t8\n");
    assert_matches_csvtk(
        &headerless,
        &["--tsv", "--no-header", "--fields", "3,1"],
        &["-tT", "--no-header-row", "--fields", "3,1"],
    );

    let (_directory, quoted) = fixture(
        "quoted.csv",
        b"id,description\n1,\"a,b\"\n2,\"left\nright\"\n",
    );
    assert_matches_csvtk(
        &quoted,
        &["--fields", "description"],
        &["--fields", "description"],
    );

    let (_directory, fuzzy) = fixture(
        "fuzzy.csv",
        b"id,\"sample,name\",sample.name,sampleXname,score1,score2\n1,S,a,b,9,8\n",
    );
    assert_matches_csvtk(
        &fuzzy,
        &["--fuzzy-fields", "--fields", "sample.name,score*"],
        &["--fuzzy-fields", "--fields", "sample.name,score*"],
    );
    assert_matches_csvtk(
        &fuzzy,
        &["--fuzzy-fields", "--fields", "-id"],
        &["--fuzzy-fields", "--fields", "-id"],
    );
}

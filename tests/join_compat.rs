use std::path::Path;
use std::process::Command;

fn outputs(input: &Path, right: &Path, ours: &[&str], upstream: &[&str]) -> (Vec<u8>, Vec<u8>) {
    let ours = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("join")
        .args(ours)
        .arg(input)
        .arg(right)
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
        .arg("join")
        .args(upstream)
        .arg(input)
        .arg(right)
        .output()
        .unwrap();
    assert_eq!(
        ours.status.success(),
        upstream.status.success(),
        "ours: {}\ncsvtk: {}",
        String::from_utf8_lossy(&ours.stderr),
        String::from_utf8_lossy(&upstream.stderr)
    );
    (ours.stdout, upstream.stdout)
}

#[test]
#[ignore = "requires pinned csvtk 0.37.0"]
fn live_csvtk_join_differential() {
    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left.csv");
    let right = directory.path().join("right.csv");
    std::fs::write(&left, b"key,left\na,L1\na,L2\nb,L3\n").unwrap();
    std::fs::write(&right, b"key,right\na,R1\na,R2\nc,R3\n").unwrap();

    let (ours, upstream) = outputs(&left, &right, &["--on", "key"], &["--fields", "key"]);
    assert_eq!(ours, upstream);

    let (ours, upstream) = outputs(
        &left,
        &right,
        &["--on", "key", "--kind", "left", "--fill", "NA"],
        &["--fields", "key", "--left-join", "--na", "NA"],
    );
    assert_eq!(ours, upstream);

    let (ours, upstream) = outputs(
        &left,
        &right,
        &["--on", "key", "--kind", "full", "--fill", "NA"],
        &["--fields", "key", "--outer-join", "--na", "NA"],
    );
    let mut ours = String::from_utf8(ours)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut upstream = String::from_utf8(upstream)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(ours.remove(0), upstream.remove(0));
    ours.sort();
    upstream.sort();
    assert_eq!(ours, upstream);
}

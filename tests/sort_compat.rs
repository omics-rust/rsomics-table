use std::path::Path;
use std::process::Command;

fn assert_matches_csvtk(input: &Path, ours: &[&str], upstream: &[&str]) {
    let ours = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("sort")
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
        .arg("sort")
        .args(upstream)
        .arg(input)
        .output()
        .unwrap();
    assert_eq!(
        ours.status.success(),
        upstream.status.success(),
        "ours: {}\ncsvtk: {}",
        String::from_utf8_lossy(&ours.stderr),
        String::from_utf8_lossy(&upstream.stderr)
    );
    assert_eq!(ours.stdout, upstream.stdout);
}

#[test]
#[ignore = "requires pinned csvtk 0.37.0"]
fn live_csvtk_sort_differential() {
    let directory = tempfile::tempdir().unwrap();
    let csv = directory.path().join("input.csv");
    std::fs::write(
        &csv,
        b"id,name,value\n1,Chr10,2\n2,chr2,9\n3,chr1,3\n4,chr2,x\n",
    )
    .unwrap();
    for (ours, upstream) in [
        (vec!["--key", "value:n"], vec!["-k", "value:n"]),
        (
            vec!["--key", "name:N", "--ignore-case"],
            vec!["-k", "name:N", "--ignore-case"],
        ),
        (
            vec!["--key", "2-3", "--no-output-header"],
            vec!["-k", "2-3", "--delete-header"],
        ),
    ] {
        assert_matches_csvtk(&csv, &ours, &upstream);
    }

    let ties = directory.path().join("ties.csv");
    let mut input = String::from("id,group,value\n");
    for index in 0..20_000 {
        input.push_str(&format!(
            "{index},group_{},{}\n",
            index % 10,
            (index * 17) % 997
        ));
    }
    std::fs::write(&ties, input).unwrap();
    assert_matches_csvtk(
        &ties,
        &["--threads", "4", "--key", "group"],
        &["--num-cpus", "4", "-k", "group"],
    );
}

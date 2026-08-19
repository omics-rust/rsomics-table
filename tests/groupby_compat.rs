use std::path::Path;
use std::process::{Command, Stdio};

fn assert_version(path: &str, arguments: &[&str], expected: &str) {
    let output = Command::new(path).args(arguments).output().unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(
        version.lines().next().unwrap().contains(expected),
        "{version}"
    );
}

fn ours(input: &Path, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rsomics-table"))
        .arg("groupby")
        .args(arguments)
        .arg(input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "requires pinned GNU datamash 1.9"]
fn live_datamash_differential() {
    let datamash =
        std::env::var("RSOMICS_DATAMASH").expect("RSOMICS_DATAMASH must name GNU datamash 1.9");
    assert_version(&datamash, &["--version"], "GNU datamash) 1.9");
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.tsv");
    std::fs::write(
        &input,
        b"sample\tgroup\tvalue\ns1\tB\t1\ns2\tA\t2\ns3\tB\t3\ns4\tA\t4\n",
    )
    .unwrap();

    let no_group = ours(
        &input,
        &[
            "--tsv",
            "-a",
            "value:sum",
            "-a",
            "value:mean",
            "-a",
            "value:sstdev",
            "-a",
            "value:median",
            "-a",
            "group:unique",
        ],
    );
    let source = std::fs::File::open(&input).unwrap();
    let upstream = Command::new(&datamash)
        .args([
            "-H", "sum", "value", "mean", "value", "sstdev", "value", "median", "value", "unique",
            "group",
        ])
        .stdin(Stdio::from(source))
        .output()
        .unwrap();
    assert!(upstream.status.success());
    assert_eq!(no_group.as_bytes(), upstream.stdout);

    let grouped = ours(
        &input,
        &[
            "--tsv",
            "--group",
            "group",
            "-a",
            "value:sum",
            "-a",
            "value:mean",
            "-a",
            "value:count",
        ],
    );
    let source = std::fs::File::open(&input).unwrap();
    let upstream = Command::new(datamash)
        .args([
            "--sort", "-H", "--group", "group", "sum", "value", "mean", "value", "count", "value",
        ])
        .stdin(Stdio::from(source))
        .output()
        .unwrap();
    assert!(upstream.status.success());
    let upstream =
        String::from_utf8(upstream.stdout)
            .unwrap()
            .replacen("GroupBy(group)", "group", 1);
    assert_eq!(grouped, upstream);
}

#[test]
#[ignore = "requires pinned bedtools 2.31.1"]
fn live_bedtools_consecutive_differential() {
    let bedtools =
        std::env::var("RSOMICS_BEDTOOLS").expect("RSOMICS_BEDTOOLS must name bedtools 2.31.1");
    assert_version(&bedtools, &["--version"], "v2.31.1");
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.tsv");
    std::fs::write(
        &input,
        b"group\tvalue\ttext\nA\t1\tb\nA\t2\ta\nA\t3\tb\nB\t4\tc\n",
    )
    .unwrap();
    let output = ours(
        &input,
        &[
            "--tsv",
            "--consecutive",
            "--group",
            "group",
            "-a",
            "value:sum",
            "-a",
            "value:mean",
            "-a",
            "value:pstdev",
            "-a",
            "value:sstdev",
            "-a",
            "value:median",
            "-a",
            "text:mode",
            "-a",
            "text:antimode",
            "-a",
            "text:collapse",
            "-a",
            "text:unique",
        ],
    );
    let upstream = Command::new(bedtools)
        .args([
            "groupby",
            "-header",
            "-g",
            "1",
            "-c",
            "2,2,2,2,2,3,3,3,3",
            "-o",
            "sum,mean,stdev,sstdev,median,mode,antimode,collapse,distinct",
            "-i",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(upstream.status.success());
    let ours = output.lines().skip(1).collect::<Vec<_>>();
    let upstream_text = String::from_utf8(upstream.stdout).unwrap();
    let upstream = upstream_text.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(ours.len(), upstream.len());
    for (ours, upstream) in ours.iter().zip(upstream) {
        let ours = ours.split('\t').collect::<Vec<_>>();
        let upstream = upstream.split('\t').collect::<Vec<_>>();
        assert_eq!(ours.len(), upstream.len());
        for index in [0, 1, 2, 5, 6, 7, 8, 9] {
            assert_eq!(ours[index], upstream[index]);
        }
        for index in [3, 4] {
            let ours = ours[index].parse::<f64>().unwrap();
            let upstream = upstream[index].parse::<f64>().unwrap_or(f64::NAN);
            assert!(
                (ours - upstream).abs() < 1e-9 || (ours.is_nan() && upstream.is_nan()),
                "{ours} != {upstream}"
            );
        }
    }
}

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::ExitCode;

const RELATIVE_TOLERANCE: f64 = 1e-9;

fn main() -> ExitCode {
    let mut arguments = env::args_os();
    let program = arguments.next().unwrap_or_default();
    let (Some(ours), Some(upstream), None) = (arguments.next(), arguments.next(), arguments.next())
    else {
        eprintln!("usage: {} OURS UPSTREAM", Path::new(&program).display());
        return ExitCode::from(2);
    };
    match compare_files(Path::new(&ours), Path::new(&upstream)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn compare_files(ours: &Path, upstream: &Path) -> Result<(), String> {
    let ours = File::open(ours).map_err(io_message)?;
    let upstream = File::open(upstream).map_err(io_message)?;
    compare(BufReader::new(ours), BufReader::new(upstream))
}

fn compare(mut ours: impl BufRead, mut upstream: impl BufRead) -> Result<(), String> {
    let mut ours_line = String::new();
    let mut upstream_line = String::new();
    let mut number = 0usize;
    loop {
        ours_line.clear();
        upstream_line.clear();
        let ours_read = ours.read_line(&mut ours_line).map_err(io_message)?;
        let upstream_read = upstream.read_line(&mut upstream_line).map_err(io_message)?;
        if ours_read == 0 && upstream_read == 0 {
            return Ok(());
        }
        number += 1;
        if ours_read == 0 || upstream_read == 0 {
            return Err(format!("row count differs at row {number}"));
        }
        compare_row(number, line_body(&ours_line), line_body(&upstream_line))?;
    }
}

fn compare_row(number: usize, ours: &str, upstream: &str) -> Result<(), String> {
    let ours = ours.split('\t').collect::<Vec<_>>();
    let upstream = upstream.split('\t').collect::<Vec<_>>();
    if ours.len() != 4 || upstream.len() != 4 {
        return Err(format!(
            "row {number} has {} and {} fields; expected four",
            ours.len(),
            upstream.len()
        ));
    }
    if ours[0] != upstream[0] {
        return Err(format!("row {number} group differs"));
    }
    if ours[3] != upstream[3] {
        return Err(format!("row {number} count differs"));
    }
    for (index, label) in [(1, "sum"), (2, "mean")] {
        let ours_number = parse_number(ours[index], number, label)?;
        let upstream_number = parse_number(upstream[index], number, label)?;
        if !equivalent_number(ours_number, upstream_number) {
            return Err(format!(
                "row {number} {label} differs: {} != {}",
                ours[index], upstream[index]
            ));
        }
    }
    Ok(())
}

fn parse_number(value: &str, row: usize, label: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|_| format!("row {row} {label} is not a number: {value:?}"))
}

fn equivalent_number(left: f64, right: f64) -> bool {
    if left.is_nan() || right.is_nan() {
        return left.is_nan() && right.is_nan();
    }
    if !left.is_finite() || !right.is_finite() {
        return left == right;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= RELATIVE_TOLERANCE * scale
}

fn line_body(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

fn io_message(error: io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn accepts_bedtools_large_number_rendering() {
        let ours = b"low_000\t24999287911\t499985.75822\t50000\n";
        let upstream = b"low_000\t2.499928791e+10\t499985.7582\t50000\n";
        assert!(compare(Cursor::new(ours), Cursor::new(upstream)).is_ok());
    }

    #[test]
    fn rejects_material_numeric_difference() {
        let ours = b"low_000\t24999287911\t499985.75822\t50000\n";
        let upstream = b"low_000\t2.499928000e+10\t499985.7582\t50000\n";
        assert!(compare(Cursor::new(ours), Cursor::new(upstream)).is_err());
    }

    #[test]
    fn requires_exact_group_count_and_row_count() {
        let ours = b"A\t1\t1\t1\nB\t2\t2\t2\n";
        assert!(compare(Cursor::new(ours), Cursor::new(b"A\t1\t1\t2\n")).is_err());
        assert!(compare(Cursor::new(ours), Cursor::new(b"C\t1\t1\t1\n")).is_err());
        assert!(compare(Cursor::new(ours), Cursor::new(b"A\t1\t1\t1\n")).is_err());
    }
}

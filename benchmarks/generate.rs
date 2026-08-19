use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

const BUFFER_SIZE: usize = 8 * 1024 * 1024;

fn main() -> io::Result<()> {
    let mut arguments = env::args_os();
    let program = arguments.next().unwrap_or_default();
    let (Some(root), Some(stream_rows), Some(sort_rows), None) = (
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
    ) else {
        usage(&program);
    };
    let Ok(stream_rows) = stream_rows.into_string() else {
        usage(&program);
    };
    let Ok(sort_rows) = sort_rows.into_string() else {
        usage(&program);
    };
    let root = PathBuf::from(root);
    let stream_rows = parse_rows(&stream_rows, "STREAM_ROWS")?;
    let sort_rows = parse_rows(&sort_rows, "SORT_ROWS")?;
    if stream_rows < 100 || sort_rows < stream_rows {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "STREAM_ROWS must be at least 100 and SORT_ROWS must not be smaller",
        ));
    }
    fs::create_dir_all(&root)?;
    write_streams(&root, stream_rows)?;
    write_sort(&root.join("sort.csv"), sort_rows)?;
    write_consecutive(&root.join("group-consecutive.tsv"), stream_rows)?;
    write_join_right(&root.join("join-right.csv"), stream_rows)?;
    Ok(())
}

fn usage(program: &OsStr) -> ! {
    eprintln!(
        "usage: {} DIRECTORY STREAM_ROWS SORT_ROWS",
        Path::new(program).display()
    );
    std::process::exit(2);
}

fn parse_rows(value: &str, name: &str) -> io::Result<u64> {
    value.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be a positive integer"),
        )
    })
}

fn writer(path: &Path) -> io::Result<BufWriter<File>> {
    Ok(BufWriter::with_capacity(BUFFER_SIZE, File::create(path)?))
}

fn write_streams(root: &Path, rows: u64) -> io::Result<()> {
    let mut csv = writer(&root.join("stream.csv"))?;
    let mut groups = writer(&root.join("group-global.tsv"))?;
    writeln!(csv, "id,low,high,value,label")?;
    let high_cardinality = (rows / 10).max(1);
    for index in 0..rows {
        let id = rows - index;
        let low = index % 100;
        let high = index % high_cardinality;
        let value = index.wrapping_mul(48_271) % 1_000_003;
        let label = index % 10_000;
        writeln!(
            csv,
            "{id},low_{low:03},high_{high:09},{value},sample_{label:04}"
        )?;
        writeln!(
            groups,
            "{id}\tlow_{low:03}\thigh_{high:09}\t{value}\tsample_{label:04}"
        )?;
    }
    Ok(())
}

fn write_sort(path: &Path, rows: u64) -> io::Result<()> {
    let mut output = writer(path)?;
    writeln!(output, "id,low,high,value,label")?;
    let high_cardinality = (rows / 10).max(1);
    for index in 0..rows {
        let id = rows - index;
        let low = index % 100;
        let high = index % high_cardinality;
        let value = index.wrapping_mul(48_271) % 1_000_003;
        let label = index % 10_000;
        writeln!(
            output,
            "{id},low_{low:03},high_{high:09},{value},sample_{label:04}"
        )?;
    }
    Ok(())
}

fn write_consecutive(path: &Path, rows: u64) -> io::Result<()> {
    let mut output = writer(path)?;
    for index in 0..rows {
        let id = index + 1;
        let low = index.saturating_mul(100) / rows;
        let high = index / 10;
        let value = index.wrapping_mul(48_271) % 1_000_003;
        let label = index % 10_000;
        writeln!(
            output,
            "{id}\tlow_{low:03}\thigh_{high:09}\t{value}\tsample_{label:04}"
        )?;
    }
    Ok(())
}

fn write_join_right(path: &Path, rows: u64) -> io::Result<()> {
    let mut output = writer(path)?;
    writeln!(output, "id,annotation")?;
    for id in (5..=rows).step_by(5) {
        writeln!(output, "{id},annotation_{:04}", id % 10_000)?;
    }
    Ok(())
}

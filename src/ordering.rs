mod natural;
mod quicksort;

use std::cmp::Ordering;

use rsomics_common::{Result, RsomicsError};

use crate::fields::{Spec, split};
use crate::io::reader::Record;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Text,
    Number,
    Natural,
}

struct ParsedKey {
    field: String,
    kind: Kind,
    reverse: bool,
}

enum Values {
    Bytes,
    Folded(Vec<String>),
    Numbers(Vec<f64>),
    Natural(Vec<String>),
    NaturalFolded(Vec<String>),
}

struct Key {
    index: usize,
    reverse: bool,
    values: Values,
}

pub(crate) struct Plan {
    keys: Vec<Key>,
}

impl Plan {
    pub(crate) fn compile(
        raw: &[String],
        width: usize,
        header: Option<&[Vec<u8>]>,
        rows: &[Record],
        ignore_case: bool,
    ) -> Result<Self> {
        let parsed = parse(raw)?;
        let mut keys = Vec::new();
        for parsed in parsed {
            let fields = Spec::parse(&parsed.field)?.resolve(width, header, false)?;
            for &index in fields.indices() {
                keys.push(Key {
                    index,
                    reverse: parsed.reverse,
                    values: prepare(parsed.kind, index, rows, ignore_case)?,
                });
            }
        }
        Ok(Self { keys })
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    pub(crate) fn sort(&self, rows: &mut [Record], threads: usize) -> Result<()> {
        if rows.len() < 2 {
            return Ok(());
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|error| {
                RsomicsError::ConfigError(format!("cannot create sort workers: {error}"))
            })?;
        let mut order = (0..rows.len()).collect::<Vec<_>>();
        pool.install(|| {
            quicksort::sort_by(&mut order, |left, right| self.less(rows, *left, *right));
        });
        let mut destinations = vec![0usize; order.len()];
        for (destination, source) in order.into_iter().enumerate() {
            destinations[source] = destination;
        }
        for source in 0..rows.len() {
            while destinations[source] != source {
                let destination = destinations[source];
                rows.swap(source, destination);
                destinations.swap(source, destination);
            }
        }
        Ok(())
    }

    fn less(&self, rows: &[Record], left: usize, right: usize) -> bool {
        for key in &self.keys {
            let ordering = key.compare(rows, left, right);
            if ordering == Ordering::Equal {
                continue;
            }
            return if key.reverse {
                ordering == Ordering::Greater
            } else {
                ordering == Ordering::Less
            };
        }
        false
    }
}

impl Key {
    fn compare(&self, rows: &[Record], left: usize, right: usize) -> Ordering {
        match &self.values {
            Values::Bytes => rows[left].fields[self.index].cmp(&rows[right].fields[self.index]),
            Values::Folded(values) => values[left].cmp(&values[right]),
            Values::Numbers(values) => {
                if values[left] < values[right] {
                    Ordering::Less
                } else if values[left] == values[right] {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            }
            Values::Natural(values) => natural_order(&values[left], &values[right]),
            Values::NaturalFolded(values) => natural_order(&values[left], &values[right]),
        }
    }
}

fn parse(raw: &[String]) -> Result<Vec<ParsedKey>> {
    let mut parsed = Vec::new();
    if raw.is_empty() {
        parsed.push(parse_one("1-")?);
        return Ok(parsed);
    }
    for raw_key in raw {
        for key in split(raw_key)? {
            parsed.push(parse_one(key)?);
        }
    }
    Ok(parsed)
}

fn parse_one(key: &str) -> Result<ParsedKey> {
    let Some((field, suffix)) = key.rsplit_once(':') else {
        return Ok(text_key(key));
    };
    if field.is_empty() {
        return Err(invalid(format!("invalid sort key {key:?}")));
    }
    let (kind, reverse) = match suffix {
        "n" => (Kind::Number, false),
        "nr" | "rn" => (Kind::Number, true),
        "N" => (Kind::Natural, false),
        "Nr" | "rN" => (Kind::Natural, true),
        "r" => (Kind::Text, true),
        "d" | "dr" => return Err(invalid("date sort keys are not supported")),
        "u" | "ur" | "ru" => return Err(invalid("custom-level sort keys are not supported")),
        _ => return Ok(text_key(key)),
    };
    Ok(ParsedKey {
        field: field.to_owned(),
        kind,
        reverse,
    })
}

fn text_key(field: &str) -> ParsedKey {
    ParsedKey {
        field: field.to_owned(),
        kind: Kind::Text,
        reverse: false,
    }
}

fn prepare(kind: Kind, index: usize, rows: &[Record], ignore_case: bool) -> Result<Values> {
    match (kind, ignore_case) {
        (Kind::Text, false) => Ok(Values::Bytes),
        (Kind::Text, true) => Ok(Values::Folded(folded(rows, index)?)),
        (Kind::Number, _) => Ok(Values::Numbers(
            rows.iter()
                .map(|record| numeric(record, index))
                .collect::<Result<Vec<_>>>()?,
        )),
        (Kind::Natural, false) => Ok(Values::Natural(texts(rows, index)?)),
        (Kind::Natural, true) => Ok(Values::NaturalFolded(folded(rows, index)?)),
    }
}

fn texts(rows: &[Record], index: usize) -> Result<Vec<String>> {
    rows.iter()
        .map(|record| Ok(text(record, index)?.to_owned()))
        .collect()
}

fn folded(rows: &[Record], index: usize) -> Result<Vec<String>> {
    rows.iter()
        .map(|record| {
            Ok(text(record, index)?
                .chars()
                .flat_map(char::to_lowercase)
                .collect())
        })
        .collect()
}

fn text(record: &Record, index: usize) -> Result<&str> {
    std::str::from_utf8(&record.fields[index]).map_err(|_| {
        invalid(format!(
            "record {}, field {}: sort key is not valid UTF-8",
            record.number,
            index + 1
        ))
    })
}

fn numeric(record: &Record, index: usize) -> Result<f64> {
    use std::borrow::Cow;

    let value = text(record, index)?;
    let cleaned = if value.contains(',') {
        Cow::Owned(value.replace(',', ""))
    } else {
        Cow::Borrowed(value)
    };
    let infinity = if ["inf", "+inf", "infinity", "+infinity"]
        .iter()
        .any(|spelling| cleaned.eq_ignore_ascii_case(spelling))
    {
        Some(f64::INFINITY)
    } else if ["-inf", "-infinity"]
        .iter()
        .any(|spelling| cleaned.eq_ignore_ascii_case(spelling))
    {
        Some(f64::NEG_INFINITY)
    } else {
        None
    };
    if let Some(value) = infinity {
        return Ok(value);
    }
    Ok(match cleaned.parse::<f64>() {
        Ok(value) if value.is_finite() => value,
        _ => f64::MAX,
    })
}

fn natural_order(left: &str, right: &str) -> Ordering {
    if left == right {
        Ordering::Equal
    } else if natural::less(left, right) {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn invalid(message: impl Into<String>) -> RsomicsError {
    RsomicsError::InvalidInput(message.into())
}

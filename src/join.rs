use std::collections::HashSet;

use rsomics_common::{Result, RsomicsError};

use crate::fields::Spec;
use crate::io::reader::Record;

pub(crate) struct Plan {
    left_keys: Vec<usize>,
    right_keys: Vec<usize>,
    right_values: Vec<usize>,
    left_sources: Vec<Option<usize>>,
    output_header: Option<Vec<Vec<u8>>>,
    ignore_case: bool,
    null_never_matches: bool,
}

pub(crate) struct Side<'a> {
    pub(crate) spec: &'a str,
    pub(crate) width: usize,
    pub(crate) header: Option<&'a [Vec<u8>]>,
}

pub(crate) struct PlanOptions<'a> {
    pub(crate) left: Side<'a>,
    pub(crate) right: Side<'a>,
    pub(crate) right_suffix: &'a str,
    pub(crate) ignore_case: bool,
    pub(crate) null_never_matches: bool,
}

impl Plan {
    pub(crate) fn compile(options: PlanOptions<'_>) -> Result<Self> {
        let left_keys = Spec::parse(options.left.spec)?
            .resolve(options.left.width, options.left.header, false)?
            .indices()
            .to_vec();
        let right_keys = Spec::parse(options.right.spec)?
            .resolve(options.right.width, options.right.header, false)?
            .indices()
            .to_vec();
        if left_keys.len() != right_keys.len() {
            return Err(invalid(format!(
                "join key arity differs: left has {}, right has {}",
                left_keys.len(),
                right_keys.len()
            )));
        }
        reject_repeated(&left_keys, "left")?;
        reject_repeated(&right_keys, "right")?;

        let right_key_set = right_keys.iter().copied().collect::<HashSet<_>>();
        let right_values = (0..options.right.width)
            .filter(|index| !right_key_set.contains(index))
            .collect::<Vec<_>>();
        let mut left_sources = vec![None; options.left.width];
        for (&left, &right) in left_keys.iter().zip(&right_keys) {
            left_sources[left] = Some(right);
        }
        let output_header = build_header(
            options.left.header,
            options.right.header,
            &right_values,
            options.right_suffix,
        )?;
        Ok(Self {
            left_keys,
            right_keys,
            right_values,
            left_sources,
            output_header,
            ignore_case: options.ignore_case,
            null_never_matches: options.null_never_matches,
        })
    }

    pub(crate) fn left_key(&self, record: &Record) -> Result<Option<Vec<u8>>> {
        self.key(record, &self.left_keys, "left")
    }

    pub(crate) fn right_key(&self, record: &Record) -> Result<Option<Vec<u8>>> {
        self.key(record, &self.right_keys, "right")
    }

    pub(crate) fn right_values(&self) -> &[usize] {
        &self.right_values
    }

    pub(crate) fn left_sources(&self) -> &[Option<usize>] {
        &self.left_sources
    }

    pub(crate) fn output_header(&self) -> Option<&[Vec<u8>]> {
        self.output_header.as_deref()
    }

    fn key(&self, record: &Record, indices: &[usize], side: &str) -> Result<Option<Vec<u8>>> {
        let mut key = Vec::new();
        for &index in indices {
            let field = &record.fields[index];
            if self.null_never_matches && field.is_empty() {
                return Ok(None);
            }
            if self.ignore_case {
                let text = std::str::from_utf8(field).map_err(|_| {
                    invalid(format!(
                        "{side} record {}, field {}: join key is not valid UTF-8",
                        record.number,
                        index + 1
                    ))
                })?;
                let folded = text
                    .chars()
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                push_component(&mut key, folded.as_bytes())?;
            } else {
                push_component(&mut key, field)?;
            }
        }
        Ok(Some(key))
    }
}

fn push_component(key: &mut Vec<u8>, field: &[u8]) -> Result<()> {
    let length = u64::try_from(field.len())
        .map_err(|_| invalid("join key field exceeds supported length"))?;
    key.extend_from_slice(&length.to_le_bytes());
    key.extend_from_slice(field);
    Ok(())
}

fn reject_repeated(indices: &[usize], side: &str) -> Result<()> {
    let mut seen = HashSet::new();
    if let Some(index) = indices.iter().find(|index| !seen.insert(**index)) {
        return Err(invalid(format!(
            "{side} join key repeats field {}",
            index + 1
        )));
    }
    Ok(())
}

fn build_header(
    left: Option<&[Vec<u8>]>,
    right: Option<&[Vec<u8>]>,
    right_values: &[usize],
    suffix: &str,
) -> Result<Option<Vec<Vec<u8>>>> {
    let (Some(left), Some(right)) = (left, right) else {
        if left.is_some() != right.is_some() {
            return Err(invalid("join inputs disagree about header presence"));
        }
        return Ok(None);
    };
    let mut output = left.to_vec();
    let mut used = left.iter().cloned().collect::<HashSet<_>>();
    for &index in right_values {
        let mut name = right[index].clone();
        if used.contains(&name) {
            let text = std::str::from_utf8(&name).map_err(|_| {
                invalid(format!(
                    "right header field {} cannot receive a text suffix",
                    index + 1
                ))
            })?;
            name = format!("{text}{suffix}").into_bytes();
            if used.contains(&name) {
                return Err(invalid(format!(
                    "joined header field {:?} is duplicated",
                    String::from_utf8_lossy(&name)
                )));
            }
        }
        used.insert(name.clone());
        output.push(name);
    }
    Ok(Some(output))
}

fn invalid(message: impl Into<String>) -> RsomicsError {
    RsomicsError::InvalidInput(message.into())
}

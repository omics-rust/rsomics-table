use std::collections::HashSet;

use regex::Regex;
use rsomics_common::{Result, RsomicsError};

#[derive(Debug)]
pub(crate) struct Spec {
    mode: Mode,
}

#[derive(Debug)]
enum Mode {
    Indices {
        selectors: Vec<IndexSelector>,
        exclusion: bool,
    },
    Names {
        selectors: Vec<String>,
        exclusion: bool,
    },
}

#[derive(Clone, Copy, Debug)]
enum IndexSelector {
    One(i64),
    Range(i64, i64),
    Open(i64),
}

impl IndexSelector {
    fn is_negative(self) -> bool {
        match self {
            Self::One(value) | Self::Open(value) | Self::Range(value, _) => value < 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Plan {
    indices: Vec<usize>,
}

impl Plan {
    pub(crate) fn indices(&self) -> &[usize] {
        &self.indices
    }

    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }
}

impl Spec {
    pub(crate) fn parse(input: &str) -> Result<Self> {
        let parts = split(input)?;
        if is_index(parts[0]) {
            parse_indices(&parts)
        } else {
            parse_names(&parts)
        }
    }

    pub(crate) fn resolve(
        &self,
        width: usize,
        header: Option<&[Vec<u8>]>,
        fuzzy: bool,
    ) -> Result<Plan> {
        let indices = match &self.mode {
            Mode::Indices {
                selectors,
                exclusion,
            } => resolve_indices(selectors, *exclusion, width)?,
            Mode::Names {
                selectors,
                exclusion,
            } => resolve_names(
                selectors,
                *exclusion,
                header.ok_or_else(|| invalid("field names require a header"))?,
                fuzzy,
            )?,
        };
        if indices.is_empty() {
            return Err(invalid("field selection matched no fields"));
        }
        Ok(Plan { indices })
    }
}

pub(crate) fn validate_header(header: &[Vec<u8>]) -> Result<()> {
    if let Some(message) = header_issues(header, false, 1).into_iter().next() {
        return Err(invalid(message));
    }
    Ok(())
}

pub(crate) fn header_issues(header: &[Vec<u8>], utf8: bool, limit: usize) -> Vec<String> {
    let mut names = HashSet::new();
    let mut issues = Vec::new();
    for (index, field) in header.iter().enumerate() {
        if field.is_empty() {
            issues.push(format!("header field {} is empty", index + 1));
        } else if !names.insert(field.as_slice()) {
            issues.push(format!("header field {} is duplicated", index + 1));
        }
        if issues.len() >= limit {
            break;
        }
        if utf8 && std::str::from_utf8(field).is_err() {
            issues.push(format!("header field {} is not valid UTF-8", index + 1));
        }
        if issues.len() >= limit {
            break;
        }
    }
    issues
}

pub(crate) fn split(input: &str) -> Result<Vec<&str>> {
    if input.is_empty() {
        return Err(invalid("field selection is empty"));
    }
    let bytes = input.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut braced = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'$' if !braced && bytes.get(index + 1) == Some(&b'{') => {
                braced = true;
                index += 1;
            }
            b'}' if braced => braced = false,
            b',' if !braced => {
                if start == index {
                    return Err(invalid("field selection contains an empty part"));
                }
                parts.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if braced {
        return Err(invalid("field name has an unclosed ${...} wrapper"));
    }
    if start == input.len() {
        return Err(invalid("field selection contains an empty part"));
    }
    parts.push(&input[start..]);
    Ok(parts)
}

fn is_index(part: &str) -> bool {
    part.bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-'))
}

fn parse_indices(parts: &[&str]) -> Result<Spec> {
    let mut selectors = Vec::new();
    let mut sign = None;
    for part in parts {
        for selector in parse_index_part(part)? {
            let negative = selector.is_negative();
            if sign.is_some_and(|existing| existing != negative) {
                return Err(invalid(
                    "positive and negative field selectors cannot be mixed",
                ));
            }
            sign = Some(negative);
            selectors.push(selector);
        }
    }
    Ok(Spec {
        mode: Mode::Indices {
            selectors,
            exclusion: sign.unwrap_or(false),
        },
    })
}

fn parse_index_part(part: &str) -> Result<Vec<IndexSelector>> {
    let separator = part
        .as_bytes()
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, byte)| (*byte == b'-').then_some(index));
    let Some(separator) = separator else {
        return Ok(vec![IndexSelector::One(parse_index_value(part)?)]);
    };
    let start = parse_index_value(&part[..separator])?;
    let end = &part[separator + 1..];
    if end.is_empty() {
        return Ok(vec![IndexSelector::Open(start)]);
    }
    let end = parse_index_value(end)?;
    if (start < 0) != (end < 0) {
        return Err(invalid(format!(
            "range endpoints must have the same sign: {part}"
        )));
    }
    if start > 0 {
        if start >= end {
            return Err(invalid(format!(
                "range start must be less than its end: {part}"
            )));
        }
        Ok(vec![IndexSelector::Range(start, end)])
    } else {
        Ok(vec![IndexSelector::Range(start, end)])
    }
}

fn parse_index_value(value: &str) -> Result<i64> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| invalid(format!("invalid field index: {value}")))?;
    if parsed == 0 {
        return Err(invalid("field indices are one-based; 0 is invalid"));
    }
    Ok(parsed)
}

fn parse_names(parts: &[&str]) -> Result<Spec> {
    let mut selectors = Vec::new();
    let mut sign = None;
    for part in parts {
        let (negative, name) = part
            .strip_prefix('-')
            .map_or((false, *part), |name| (true, name));
        if sign.is_some_and(|existing| existing != negative) {
            return Err(invalid(
                "positive and negative field selectors cannot be mixed",
            ));
        }
        sign = Some(negative);
        let name = unwrap_name(name)?;
        if name.is_empty() {
            return Err(invalid("field name is empty"));
        }
        selectors.push(name.to_owned());
    }
    Ok(Spec {
        mode: Mode::Names {
            selectors,
            exclusion: sign.unwrap_or(false),
        },
    })
}

fn unwrap_name(name: &str) -> Result<&str> {
    if let Some(name) = name.strip_prefix("${") {
        return name
            .strip_suffix('}')
            .ok_or_else(|| invalid("field name has an unclosed ${...} wrapper"));
    }
    Ok(name)
}

fn resolve_indices(
    selectors: &[IndexSelector],
    exclusion: bool,
    width: usize,
) -> Result<Vec<usize>> {
    let mut resolved = Vec::new();
    for selector in selectors {
        match *selector {
            IndexSelector::One(value) => {
                let index = absolute_index(value)?;
                if index >= width {
                    return Err(out_of_range(index, width));
                }
                resolved.push(index);
            }
            IndexSelector::Open(value) => {
                let index = absolute_index(value)?;
                if index >= width {
                    return Err(out_of_range(index, width));
                }
                resolved.extend(index..width);
            }
            IndexSelector::Range(start, end) => {
                let start = absolute_index(start)?;
                let end = absolute_index(end)?;
                if start >= width {
                    return Err(out_of_range(start, width));
                }
                if end >= width {
                    return Err(out_of_range(end, width));
                }
                let low = start.min(end);
                let high = start.max(end);
                resolved.extend(low..=high);
            }
        }
    }
    if !exclusion {
        return Ok(resolved);
    }
    let excluded: HashSet<usize> = resolved.into_iter().collect();
    Ok((0..width)
        .filter(|index| !excluded.contains(index))
        .collect())
}

fn absolute_index(value: i64) -> Result<usize> {
    let value = value.unsigned_abs();
    usize::try_from(value - 1).map_err(|_| invalid("field index exceeds platform limits"))
}

fn out_of_range(index: usize, width: usize) -> RsomicsError {
    invalid(format!(
        "field {} is out of range for {width} fields",
        index + 1
    ))
}

fn resolve_names(
    selectors: &[String],
    exclusion: bool,
    header: &[Vec<u8>],
    fuzzy: bool,
) -> Result<Vec<usize>> {
    if fuzzy {
        let names = header
            .iter()
            .enumerate()
            .map(|(index, name)| {
                std::str::from_utf8(name)
                    .map_err(|_| invalid(format!("header field {} is not valid UTF-8", index + 1)))
            })
            .collect::<Result<Vec<_>>>()?;
        return resolve_fuzzy(selectors, exclusion, &names);
    }
    let selected = selectors
        .iter()
        .map(|selector| {
            header
                .iter()
                .position(|name| name.as_slice() == selector.as_bytes())
                .ok_or_else(|| invalid(format!("field {selector:?} is not present")))
        })
        .collect::<Result<Vec<_>>>()?;
    if !exclusion {
        return Ok(selected);
    }
    let excluded: HashSet<usize> = selected.into_iter().collect();
    Ok((0..header.len())
        .filter(|index| !excluded.contains(index))
        .collect())
}

fn resolve_fuzzy(selectors: &[String], exclusion: bool, header: &[&str]) -> Result<Vec<usize>> {
    let patterns = selectors
        .iter()
        .map(|selector| {
            Regex::new(&format!("^{}$", selector.replace('*', ".*?")))
                .map_err(|error| invalid(format!("invalid fuzzy field {selector:?}: {error}")))
        })
        .collect::<Result<Vec<_>>>()?;
    if exclusion {
        let mut matched = false;
        let kept = header
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let excluded = patterns.iter().any(|pattern| pattern.is_match(name));
                matched |= excluded;
                (!excluded).then_some(index)
            })
            .collect::<Vec<_>>();
        if !matched {
            return Err(invalid("fuzzy field selection matched no fields"));
        }
        return Ok(kept);
    }
    let mut selected = Vec::new();
    for pattern in patterns {
        selected.extend(
            header
                .iter()
                .enumerate()
                .filter_map(|(index, name)| pattern.is_match(name).then_some(index)),
        );
    }
    if selected.is_empty() {
        return Err(invalid("fuzzy field selection matched no fields"));
    }
    Ok(selected)
}

fn invalid(message: impl Into<String>) -> RsomicsError {
    RsomicsError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<Vec<u8>> {
        ["id", "name", "score", "sample name"]
            .map(|name| name.as_bytes().to_vec())
            .to_vec()
    }

    fn resolve(spec: &str) -> Vec<usize> {
        Spec::parse(spec)
            .unwrap()
            .resolve(4, Some(&header()), false)
            .unwrap()
            .indices
    }

    #[test]
    fn indices_ranges_repeats_and_exclusions() {
        assert_eq!(resolve("3,1,3"), [2, 0, 2]);
        assert_eq!(resolve("2-,1"), [1, 2, 3, 0]);
        assert_eq!(resolve("-2--3"), [0, 3]);
    }

    #[test]
    fn names_and_braced_names_preserve_order() {
        assert_eq!(resolve("score,id,score"), [2, 0, 2]);
        assert_eq!(resolve("${sample name},id"), [3, 0]);
    }

    #[test]
    fn rejects_invalid_and_mixed_selectors() {
        for spec in ["", "0", "1,-2", "name,-score", "3-2", "9", "missing"] {
            let parsed = Spec::parse(spec);
            if let Ok(parsed) = parsed {
                assert!(parsed.resolve(4, Some(&header()), false).is_err(), "{spec}");
            }
        }
    }

    #[test]
    fn fuzzy_selection_uses_pattern_order() {
        let plan = Spec::parse("s*,id")
            .unwrap()
            .resolve(4, Some(&header()), true)
            .unwrap();
        assert_eq!(plan.indices, [2, 3, 0]);
    }

    #[test]
    fn range_plans_do_not_expand_before_width_resolution() {
        let spec = Spec::parse("1-1000").unwrap();
        let Mode::Indices { selectors, .. } = spec.mode else {
            panic!("expected index selectors");
        };
        assert_eq!(selectors.len(), 1);
    }
}

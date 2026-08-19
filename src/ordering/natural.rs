fn chunks(value: &str) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut chunks = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let start = cursor;
        let digits = bytes[cursor].is_ascii_digit();
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() == digits {
            cursor += char_width(bytes[cursor]);
        }
        chunks.push(&value[start..cursor]);
    }
    chunks
}

fn char_width(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

pub(super) fn less(left: &str, right: &str) -> bool {
    if left.is_empty() {
        return true;
    }
    if right.is_empty() {
        return false;
    }
    let left_chunks = chunks(left);
    let right_chunks = chunks(right);
    for (index, left_chunk) in left_chunks.iter().enumerate() {
        let Some(right_chunk) = right_chunks.get(index) else {
            return false;
        };
        if let (Ok(left_number), Ok(right_number)) =
            (left_chunk.parse::<i64>(), right_chunk.parse::<i64>())
        {
            if left_number == right_number {
                if index == left_chunks.len() - 1 {
                    return true;
                }
                if index == right_chunks.len() - 1 {
                    return false;
                }
                continue;
            }
            return left_number < right_number;
        }
        if left_chunk == right_chunk {
            if index == left_chunks.len() - 1 {
                return true;
            }
            if index == right_chunks.len() - 1 {
                return false;
            }
            continue;
        }
        return left_chunk < right_chunk;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numeric_chunks_and_unicode_text() {
        assert!(less("chr2", "chr10"));
        assert!(less("café2", "café10"));
        assert!(!less("chr10", "chr2"));
    }
}

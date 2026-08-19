const SMALL: usize = 127;
const PARALLEL: usize = 10_000;

pub(super) fn sort_by<T, F>(values: &mut [T], less: F)
where
    T: Send,
    F: Fn(&T, &T) -> bool + Sync,
{
    if values.len() < 2 {
        return;
    }
    worker(values, &less, 2 * bit_len(values.len()) + 2);
}

fn bit_len(mut value: usize) -> usize {
    let mut length = 0;
    while value > 0 {
        length += 1;
        value >>= 1;
    }
    length
}

fn worker<T, F>(values: &mut [T], less: &F, mut depth: usize)
where
    T: Send,
    F: Fn(&T, &T) -> bool + Sync,
{
    if values.len() <= SMALL {
        if values.len() > 7 {
            quick_sort(values, less, 0, values.len(), depth);
        } else if values.len() > 1 {
            insertion_sort(values, less, 0, values.len());
        }
        return;
    }
    if depth == 0 {
        heap_sort(values, less, 0, values.len());
        return;
    }
    depth -= 1;
    let length = values.len();
    let (middle_low, middle_high) = pivot(values, less, 0, length);
    let left_length = middle_low;
    let right_length = length - middle_high;
    let (left_depth, right_depth) = if left_length < right_length {
        (depth + 2, depth)
    } else {
        (depth, depth + 2)
    };
    let (left, remainder) = values.split_at_mut(middle_low);
    let (_, right) = remainder.split_at_mut(middle_high - middle_low);
    if length >= PARALLEL {
        rayon::join(
            || worker(left, less, left_depth),
            || worker(right, less, right_depth),
        );
    } else {
        worker(left, less, left_depth);
        worker(right, less, right_depth);
    }
}

fn before<T, F>(values: &[T], less: &F, left: usize, right: usize) -> bool
where
    F: Fn(&T, &T) -> bool,
{
    less(&values[left], &values[right])
}

fn insertion_sort<T, F>(values: &mut [T], less: &F, start: usize, end: usize)
where
    F: Fn(&T, &T) -> bool,
{
    for index in (start + 1)..end {
        let mut cursor = index;
        while cursor > start && before(values, less, cursor, cursor - 1) {
            values.swap(cursor, cursor - 1);
            cursor -= 1;
        }
    }
}

fn sift_down<T, F>(values: &mut [T], less: &F, low: usize, high: usize, first: usize)
where
    F: Fn(&T, &T) -> bool,
{
    let mut root = low;
    loop {
        let mut child = 2 * root + 1;
        if child >= high {
            break;
        }
        if child + 1 < high && before(values, less, first + child, first + child + 1) {
            child += 1;
        }
        if !before(values, less, first + root, first + child) {
            return;
        }
        values.swap(first + root, first + child);
        root = child;
    }
}

fn heap_sort<T, F>(values: &mut [T], less: &F, start: usize, end: usize)
where
    F: Fn(&T, &T) -> bool,
{
    let length = end - start;
    let mut index = (length as isize - 1) / 2;
    while index >= 0 {
        sift_down(values, less, index as usize, length, start);
        index -= 1;
    }
    let mut index = length as isize - 1;
    while index >= 0 {
        values.swap(start, start + index as usize);
        sift_down(values, less, 0, index as usize, start);
        index -= 1;
    }
}

fn median<T, F>(values: &mut [T], less: &F, left: usize, middle: usize, right: usize)
where
    F: Fn(&T, &T) -> bool,
{
    if before(values, less, left, middle) {
        values.swap(left, middle);
    }
    if before(values, less, right, left) {
        values.swap(right, left);
        if before(values, less, left, middle) {
            values.swap(left, middle);
        }
    }
}

fn pivot<T, F>(values: &mut [T], less: &F, low: usize, high: usize) -> (usize, usize)
where
    F: Fn(&T, &T) -> bool,
{
    let middle = low + (high - low) / 2;
    if high - low > 40 {
        let step = (high - low) / 8;
        median(values, less, low, low + step, low + 2 * step);
        median(values, less, middle, middle - step, middle + step);
        median(values, less, high - 1, high - 1 - step, high - 1 - 2 * step);
    }
    median(values, less, low, middle, high - 1);
    let pivot = low;
    let (mut left, mut right) = (low + 1, high - 1);
    while left != right && before(values, less, left, pivot) {
        left += 1;
    }
    let mut scan = left;
    loop {
        while scan != right && !before(values, less, pivot, scan) {
            scan += 1;
        }
        while scan != right && before(values, less, pivot, right - 1) {
            right -= 1;
        }
        if scan == right {
            break;
        }
        values.swap(scan, right - 1);
        scan += 1;
        right -= 1;
    }
    let mut protect = high - right < 5;
    if !protect && high - right < (high - low) / 4 {
        let mut duplicates = 0;
        if !before(values, less, pivot, high - 1) {
            values.swap(right, high - 1);
            right += 1;
            duplicates += 1;
        }
        if !before(values, less, scan - 1, pivot) {
            scan -= 1;
            duplicates += 1;
        }
        if !before(values, less, middle, pivot) {
            values.swap(middle, scan - 1);
            scan -= 1;
            duplicates += 1;
        }
        protect = duplicates > 1;
    }
    if protect {
        loop {
            while left != scan && !before(values, less, scan - 1, pivot) {
                scan -= 1;
            }
            while left != scan && before(values, less, left, pivot) {
                left += 1;
            }
            if left == scan {
                break;
            }
            values.swap(left, scan - 1);
            left += 1;
            scan -= 1;
        }
    }
    values.swap(pivot, scan - 1);
    (scan - 1, right)
}

fn quick_sort<T, F>(values: &mut [T], less: &F, mut start: usize, mut end: usize, mut depth: usize)
where
    F: Fn(&T, &T) -> bool,
{
    while end - start > 12 {
        if depth == 0 {
            heap_sort(values, less, start, end);
            return;
        }
        depth -= 1;
        let (middle_low, middle_high) = pivot(values, less, start, end);
        if middle_low - start < end - middle_high {
            quick_sort(values, less, start, middle_low, depth);
            start = middle_high;
        } else {
            quick_sort(values, less, middle_high, end, depth);
            end = middle_low;
        }
    }
    if end - start > 1 {
        for index in (start + 6)..end {
            if before(values, less, index, index - 6) {
                values.swap(index, index - 6);
            }
        }
        insertion_sort(values, less, start, end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_sizes_and_duplicate_values() {
        for size in [0usize, 1, 2, 7, 8, 12, 13, 40, 41, 128, 200, 1_000] {
            let mut values = (0..size as i64).rev().collect::<Vec<_>>();
            let mut expected = values.clone();
            sort_by(&mut values, |left, right| left < right);
            expected.sort_unstable();
            assert_eq!(values, expected, "size {size}");
        }

        let mut values = (0..500).map(|value| value % 7).collect::<Vec<_>>();
        let mut expected = values.clone();
        sort_by(&mut values, |left, right| left < right);
        expected.sort_unstable();
        assert_eq!(values, expected);
    }
}

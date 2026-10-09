//! The original's `std::sort`: the Visual C++ 2008 library's introsort, for the places where the
//! game sorts keys that can be equal and the order of equal keys changes the result.
//!
//! CONFIRMED from the exe's instance for (key, value) pairs sorted by value, largest first
//! (`0x00B7F790`, used by the trade builder `0x00BC0960` and the trade split `0x00BC26D0`):
//! - while more than 32 elements are left and the depth budget (starting at the element count) is
//!   above 0: partition around a median pivot (`0x00B7DDB0`; median of three, or of three medians
//!   of three with step `(n + 1) / 8` when the range spans more than 40 elements, `0x00B7D570`),
//!   shrink the budget to `budget / 2 + budget / 2 / 2`, sort the smaller part recursively and
//!   loop on the larger;
//! - a range left with more than 32 elements and no budget is heap-sorted (`0x00B7F090` is the
//!   sift-down / sift-up step);
//! - a range of 2..=32 elements is insertion-sorted, which keeps equal elements in their order.

/// The library's insertion-sort threshold (`_ISORT_MAX`).
const ISORT_MAX: usize = 32;

/// Sorts `v` as the original's `std::sort` does with the strict "goes before" test `less`.
pub fn sort_by<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], less: F) {
    let n = v.len();
    sort_range(v, 0, n, n, &less);
}

fn sort_range<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], mut first: usize, mut last: usize, mut budget: usize, less: &F) {
    while last - first > ISORT_MAX && budget > 0 {
        let (low, high) = partition(v, first, last, less);
        budget = (budget >> 1) + (budget >> 2);
        if low - first < last - high {
            sort_range(v, first, low, budget, less);
            first = high;
        } else {
            sort_range(v, high, last, budget, less);
            last = low;
        }
    }
    let part = &mut v[first..last];
    if part.len() > ISORT_MAX {
        heap_sort(part, less);
    } else if part.len() > 1 {
        insertion_sort(part, less);
    }
}

/// Orders three elements so the median is at `b`.
fn med3<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], a: usize, b: usize, c: usize, less: &F) {
    if less(&v[b], &v[a]) {
        v.swap(a, b);
    }
    if less(&v[c], &v[b]) {
        v.swap(b, c);
        if less(&v[b], &v[a]) {
            v.swap(a, b);
        }
    }
}

/// Moves the pivot candidate to `mid`; `last` is the range's last element (inclusive).
fn median<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, mid: usize, last: usize, less: &F) {
    if last - first > 40 {
        let step = (last - first + 1) / 8;
        med3(v, first, first + step, first + 2 * step, less);
        med3(v, mid - step, mid, mid + step, less);
        med3(v, last - 2 * step, last - step, last, less);
        med3(v, first + step, mid, last - step, less);
    } else {
        med3(v, first, mid, last, less);
    }
}

/// Three-way partition of `first..last`: returns the range holding the elements equal to the pivot.
fn partition<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, last: usize, less: &F) -> (usize, usize) {
    let equal = |v: &[T], a: usize, b: usize| !less(&v[a], &v[b]) && !less(&v[b], &v[a]);
    let mid = first + (last - first) / 2;
    median(v, first, mid, last - 1, less);
    let mut pfirst = mid;
    let mut plast = pfirst + 1;
    while first < pfirst && equal(v, pfirst - 1, pfirst) {
        pfirst -= 1;
    }
    while plast < last && equal(v, plast, pfirst) {
        plast += 1;
    }
    let mut gfirst = plast;
    let mut glast = pfirst;
    loop {
        while gfirst < last {
            if less(&v[pfirst], &v[gfirst]) {
            } else if less(&v[gfirst], &v[pfirst]) {
                break;
            } else {
                v.swap(plast, gfirst);
                plast += 1;
            }
            gfirst += 1;
        }
        while first < glast {
            if less(&v[glast - 1], &v[pfirst]) {
            } else if less(&v[pfirst], &v[glast - 1]) {
                break;
            } else {
                pfirst -= 1;
                v.swap(pfirst, glast - 1);
            }
            glast -= 1;
        }
        if glast == first && gfirst == last {
            return (pfirst, plast);
        }
        if glast == first {
            // No room below: rotate the pivot range up.
            if plast != gfirst {
                v.swap(pfirst, plast);
            }
            plast += 1;
            v.swap(pfirst, gfirst);
            pfirst += 1;
            gfirst += 1;
        } else if gfirst == last {
            // No room above: rotate the pivot range down.
            glast -= 1;
            pfirst -= 1;
            if glast != pfirst {
                v.swap(glast, pfirst);
            }
            plast -= 1;
            v.swap(pfirst, plast);
        } else {
            glast -= 1;
            v.swap(gfirst, glast);
            gfirst += 1;
        }
    }
}

fn insertion_sort<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], less: &F) {
    for next in 1..v.len() {
        let val = v[next];
        if less(&val, &v[0]) {
            v.copy_within(0..next, 1);
            v[0] = val;
        } else {
            let mut hole = next;
            while less(&val, &v[hole - 1]) {
                v[hole] = v[hole - 1];
                hole -= 1;
            }
            v[hole] = val;
        }
    }
}

/// Sift `val` down from `hole` within the first `bottom` elements, then back up towards `top`.
fn adjust_heap<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], mut hole: usize, bottom: usize, val: T, less: &F) {
    let top = hole;
    let mut idx = 2 * hole + 2;
    while idx < bottom {
        if less(&v[idx], &v[idx - 1]) {
            idx -= 1;
        }
        v[hole] = v[idx];
        hole = idx;
        idx = 2 * idx + 2;
    }
    if idx == bottom {
        v[hole] = v[bottom - 1];
        hole = bottom - 1;
    }
    while top < hole {
        let parent = (hole - 1) / 2;
        if !less(&v[parent], &val) {
            break;
        }
        v[hole] = v[parent];
        hole = parent;
    }
    v[hole] = val;
}

fn heap_sort<T: Copy, F: Fn(&T, &T) -> bool>(v: &mut [T], less: &F) {
    let n = v.len();
    let mut hole = n / 2;
    while hole > 0 {
        hole -= 1;
        adjust_heap(v, hole, n, v[hole], less);
    }
    let mut last = n;
    while last > 1 {
        last -= 1;
        let val = v[last];
        v[last] = v[0];
        adjust_heap(v, 0, last, val, less);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exe's comparator for (key, value) pairs: larger value first.
    fn by_value_desc(v: &mut [(u32, i32)]) {
        sort_by(v, |a, b| a.1 > b.1);
    }

    /// A small deterministic generator for the test inputs.
    fn values(n: usize, modulo: u32, seed: u32) -> Vec<(u32, i32)> {
        let mut s = seed;
        (0..n as u32)
            .map(|k| {
                s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (k, ((s >> 16) % modulo) as i32)
            })
            .collect()
    }

    #[test]
    fn every_size_comes_out_sorted_and_complete() {
        for n in 0..300 {
            for modulo in [2, 5, 1000] {
                let mut v = values(n, modulo, n as u32 * 7 + modulo);
                by_value_desc(&mut v);
                assert!(v.windows(2).all(|w| w[0].1 >= w[1].1), "n {n} modulo {modulo}");
                let mut keys: Vec<u32> = v.iter().map(|p| p.0).collect();
                keys.sort_unstable();
                assert_eq!(keys, (0..n as u32).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn up_to_32_elements_keep_equal_keys_in_order() {
        for n in 0..=32 {
            let mut v = values(n, 3, 99 + n as u32);
            let mut stable = v.clone();
            stable.sort_by_key(|a| std::cmp::Reverse(a.1));
            by_value_desc(&mut v);
            assert_eq!(v, stable, "n {n}");
        }
    }

    #[test]
    fn longer_ranges_order_equal_keys_by_the_partition() {
        // 33 elements is past the insertion threshold: the partition step decides the order of the
        // equal keys, so it is no longer the input order.
        let mut v: Vec<(u32, i32)> = (0..33).map(|k| (k, (k % 2) as i32)).collect();
        by_value_desc(&mut v);
        let ones: Vec<u32> = v.iter().filter(|p| p.1 == 1).map(|p| p.0).collect();
        let zeros: Vec<u32> = v.iter().filter(|p| p.1 == 0).map(|p| p.0).collect();
        assert_eq!(ones.len(), 16);
        assert_eq!(zeros.len(), 17);
        assert_ne!(zeros, (0..33).step_by(2).collect::<Vec<u32>>(), "a stable sort would keep the input order");
    }
}

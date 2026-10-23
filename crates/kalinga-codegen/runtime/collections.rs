#[inline(always)]
fn __idx(i: i64) -> usize {
    if i < 0 {
        panic!("index {} is negative", i)
    }
    i as usize
}
fn __map_get<K: Ord + __KlShow, V: Clone>(m: &BTreeMap<K, V>, k: &K) -> V {
    match m.get(k) {
        Some(v) => v.clone(),
        None => panic!("key not found: {}", k.__show()),
    }
}
fn __map_slot<K: Ord + __KlShow, V>(m: &mut BTreeMap<K, V>, k: K) -> &mut V {
    let shown = k.__show();
    match m.get_mut(&k) {
        Some(v) => v,
        None => panic!("key not found: {}", shown),
    }
}
fn __clamp<T: PartialOrd>(x: T, lo: T, hi: T) -> T {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

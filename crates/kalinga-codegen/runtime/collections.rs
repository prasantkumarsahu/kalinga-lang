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
/// `s[i]`: the character at index `i`, as a string.
fn __char_at(s: &str, i: i64) -> String {
    match usize::try_from(i).ok().and_then(|i| s.chars().nth(i)) {
        Some(c) => c.to_string(),
        None => panic!("index out of bounds: the string has {} characters but the index is {}", s.chars().count(), i),
    }
}
/// The checked range of a slice `from..to` of something `len` long.
fn __slice_range(len: usize, from: Option<i64>, to: Option<i64>) -> (usize, usize) {
    let (a, b) = (from.unwrap_or(0), to.unwrap_or(len as i64));
    if a < 0 || b < a || b as usize > len {
        panic!("slice {}..{} is out of bounds for length {}", a, b, len)
    }
    (a as usize, b as usize)
}
fn __slice_str(s: &str, from: Option<i64>, to: Option<i64>) -> String {
    let (a, b) = __slice_range(s.chars().count(), from, to);
    s.chars().skip(a).take(b - a).collect()
}
fn __slice_vec<T: Clone>(v: &[T], from: Option<i64>, to: Option<i64>) -> Vec<T> {
    let (a, b) = __slice_range(v.len(), from, to);
    v[a..b].to_vec()
}
/// `s.split(sep)`; an empty separator splits into characters.
fn __split(s: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() { s.chars().map(|c| c.to_string()).collect() } else { s.split(sep).map(String::from).collect() }
}
/// `s.find(x)`: the character index of the first `x` in `s`.
fn __str_find(s: &str, x: &str) -> Option<i64> {
    s.find(x).map(|byte| s[..byte].chars().count() as i64)
}
/// `s.code_at(i)`: the code point of the character at index `i`.
fn __code_at(s: &str, i: i64) -> i64 {
    match usize::try_from(i).ok().and_then(|i| s.chars().nth(i)) {
        Some(c) => c as i64,
        None => panic!("index out of bounds: the string has {} characters but the index is {}", s.chars().count(), i),
    }
}
/// `chr(code)`: the character with code point `code`.
fn __chr(code: i64) -> String {
    match u32::try_from(code).ok().and_then(char::from_u32) {
        Some(c) => c.to_string(),
        None => panic!("{} is not a character code", code),
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

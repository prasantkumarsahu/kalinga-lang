// `==` and ordering for values that hold objects, and for type parameters with `Eq` or
// `Ord` bounds. Rust's own `PartialEq` cannot be given to `Rc<RefCell<Class>>` here, so
// comparisons that may reach an object go through these traits; the compiler writes the
// impls for classes (calling `equals` / `compare`) and for structs and enums.
trait __KlEq {
    fn __eq(&self, o: &Self) -> bool;
}
trait __KlOrd {
    fn __cmp(&self, o: &Self) -> std::cmp::Ordering;
}
impl __KlEq for i64 {
    fn __eq(&self, o: &Self) -> bool {
        self == o
    }
}
impl __KlEq for f64 {
    fn __eq(&self, o: &Self) -> bool {
        self == o
    }
}
impl __KlEq for bool {
    fn __eq(&self, o: &Self) -> bool {
        self == o
    }
}
impl __KlEq for String {
    fn __eq(&self, o: &Self) -> bool {
        self == o
    }
}
impl __KlEq for () {
    fn __eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T: __KlEq> __KlEq for Vec<T> {
    fn __eq(&self, o: &Self) -> bool {
        self.len() == o.len() && self.iter().zip(o).all(|(a, b)| a.__eq(b))
    }
}
impl<T: __KlEq> __KlEq for Option<T> {
    fn __eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Some(a), Some(b)) => a.__eq(b),
            (None, None) => true,
            _ => false,
        }
    }
}
impl<T: __KlEq + ?Sized> __KlEq for Box<T> {
    fn __eq(&self, o: &Self) -> bool {
        (**self).__eq(o)
    }
}
impl<K: PartialEq, V: __KlEq> __KlEq for std::collections::BTreeMap<K, V> {
    fn __eq(&self, o: &Self) -> bool {
        self.len() == o.len() && self.iter().zip(o).all(|((k1, v1), (k2, v2))| k1 == k2 && v1.__eq(v2))
    }
}
impl<T: __KlEq, E: __KlEq> __KlEq for Result<T, E> {
    fn __eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Ok(a), Ok(b)) => a.__eq(b),
            (Err(a), Err(b)) => a.__eq(b),
            _ => false,
        }
    }
}
macro_rules! __kl_eq_tuple {
    ($($t:ident $i:tt),+) => {
        impl<$($t: __KlEq),+> __KlEq for ($($t,)+) {
            fn __eq(&self, o: &Self) -> bool {
                $(self.$i.__eq(&o.$i))&&+
            }
        }
    };
}
__kl_eq_tuple!(A 0);
__kl_eq_tuple!(A 0, B 1);
__kl_eq_tuple!(A 0, B 1, C 2);
__kl_eq_tuple!(A 0, B 1, C 2, D 3);
__kl_eq_tuple!(A 0, B 1, C 2, D 3, E 4);
__kl_eq_tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
__kl_eq_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
__kl_eq_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
impl __KlOrd for i64 {
    fn __cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.cmp(o)
    }
}
impl __KlOrd for f64 {
    fn __cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.total_cmp(o)
    }
}
impl __KlOrd for bool {
    fn __cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.cmp(o)
    }
}
impl __KlOrd for String {
    fn __cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.cmp(o)
    }
}

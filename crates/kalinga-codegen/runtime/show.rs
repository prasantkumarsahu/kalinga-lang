trait __KlShow {
    fn __show(&self) -> String;
}
impl __KlShow for i64 {
    fn __show(&self) -> String {
        self.to_string()
    }
}
impl __KlShow for f64 {
    fn __show(&self) -> String {
        if self.is_finite() && self.fract() == 0.0 && self.abs() < 1e16 {
            format!("{:.1}", self)
        } else {
            self.to_string()
        }
    }
}
impl __KlShow for bool {
    fn __show(&self) -> String {
        self.to_string()
    }
}
impl __KlShow for String {
    fn __show(&self) -> String {
        self.clone()
    }
}
impl<T: __KlShow> __KlShow for Vec<T> {
    fn __show(&self) -> String {
        let parts: Vec<String> = self.iter().map(|v| v.__show()).collect();
        format!("[{}]", parts.join(", "))
    }
}
impl<K: __KlShow, V: __KlShow> __KlShow for BTreeMap<K, V> {
    fn __show(&self) -> String {
        let parts: Vec<String> = self.iter().map(|(k, v)| format!("{}: {}", k.__show(), v.__show())).collect();
        format!("{{{}}}", parts.join(", "))
    }
}
impl<T: __KlShow, E: __KlShow> __KlShow for Result<T, E> {
    fn __show(&self) -> String {
        match self {
            Ok(v) => format!("Ok({})", v.__show()),
            Err(e) => format!("Err({})", e.__show()),
        }
    }
}
impl __KlShow for () {
    fn __show(&self) -> String {
        String::new()
    }
}
impl<T: __KlShow> __KlShow for Box<T> {
    fn __show(&self) -> String {
        (**self).__show()
    }
}
// Tuples print as `(1, a)`.
macro_rules! __kl_show_tuple {
    ($($t:ident $i:tt),+) => {
        impl<$($t: __KlShow),+> __KlShow for ($($t,)+) {
            fn __show(&self) -> String {
                let parts: Vec<String> = vec![$(self.$i.__show()),+];
                format!("({})", parts.join(", "))
            }
        }
    };
}
__kl_show_tuple!(A 0, B 1);
__kl_show_tuple!(A 0, B 1, C 2);
__kl_show_tuple!(A 0, B 1, C 2, D 3);
__kl_show_tuple!(A 0, B 1, C 2, D 3, E 4);
__kl_show_tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
impl<T: __KlShow> __KlShow for Option<T> {
    fn __show(&self) -> String {
        match self {
            Some(v) => v.__show(),
            None => "none".to_string(),
        }
    }
}

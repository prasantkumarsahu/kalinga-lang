// Rust side of `std/reactive`.

/// Stops the program: a signal or computed value was used after its scope was disposed.
pub fn reactive_disposed() {
    panic!("this reactive value was disposed (its scope has ended)");
}

// Rust helpers called from examples/interop/main.kl.

/// Mean and standard deviation, computed in Rust.
pub fn mean(xs: Vec<f64>) -> f64 {
    if xs.is_empty() { 0.0 } else { xs.iter().sum::<f64>() / xs.len() as f64 }
}

pub fn std_dev(xs: Vec<f64>) -> f64 {
    let m = mean(xs.clone());
    (xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / xs.len().max(1) as f64).sqrt()
}

/// An opaque Rust type: a fixed-size matrix of floats.
#[derive(Clone)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

pub fn matrix_new(rows: i64, cols: i64, fill: f64) -> Matrix {
    Matrix { rows: rows as usize, cols: cols as usize, data: vec![fill; (rows * cols) as usize] }
}

pub fn matrix_set(mut m: Matrix, r: i64, c: i64, v: f64) -> Matrix {
    let i = r as usize * m.cols + c as usize;
    m.data[i] = v;
    m
}

pub fn matrix_mul(a: Matrix, b: Matrix) -> Matrix {
    assert!(a.cols == b.rows, "matrix sizes do not match");
    let mut out = vec![0.0; a.rows * b.cols];
    for i in 0..a.rows {
        for k in 0..a.cols {
            let x = a.data[i * a.cols + k];
            for j in 0..b.cols {
                out[i * b.cols + j] += x * b.data[k * b.cols + j];
            }
        }
    }
    Matrix { rows: a.rows, cols: b.cols, data: out }
}

pub fn matrix_rows(m: Matrix) -> Vec<Vec<f64>> {
    m.data.chunks(m.cols).map(|r| r.to_vec()).collect()
}

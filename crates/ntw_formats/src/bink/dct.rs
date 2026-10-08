//! The two transforms of Bink audio, written as our own FFT-based code.
//!
//! The original computes them with a split-radix float FFT (`BINK.md` §5.3); these give the
//! same mathematical result in f64. Float rounding can differ in the last bit, so a 16-bit sample
//! may differ by 1 from the original's in rare cases (INFERRED; not measured against the DLL).

use std::f64::consts::PI;

/// In-place iterative radix-2 complex FFT with `e^{+i...}` (unscaled inverse DFT).
struct Fft {
    n: usize,
    /// `e^{+2*pi*i*k/n}` for k < n/2.
    twiddle: Vec<(f64, f64)>,
    rev: Vec<u32>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let rev = (0..n as u32).map(|i| i.reverse_bits() >> (32 - bits)).collect();
        let twiddle = (0..n / 2).map(|k| {
            let a = 2.0 * PI * k as f64 / n as f64;
            (a.cos(), a.sin())
        });
        Fft { n, twiddle: twiddle.collect(), rev }
    }

    fn run(&self, re: &mut [f64], im: &mut [f64]) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = self.twiddle[k * step];
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

/// `C[k] = sum_{j<n} a[j] * cos(pi * j * (k + 1/2) / n)` (DCT-III without the half weight on
/// `a[0]`; the definition the original's DCT routine computes, `BINK.md` §5.3).
pub(crate) struct Dct3 {
    n: usize,
    fft: Fft,
    /// `e^{i*pi*j/(2n)}` for j < n.
    pre: Vec<(f64, f64)>,
    re: Vec<f64>,
    im: Vec<f64>,
}

impl Dct3 {
    pub(crate) fn new(n: usize) -> Dct3 {
        let pre = (0..n).map(|j| {
            let a = PI * j as f64 / (2 * n) as f64;
            (a.cos(), a.sin())
        });
        Dct3 { n, fft: Fft::new(2 * n), pre: pre.collect(), re: vec![0.0; 2 * n], im: vec![0.0; 2 * n] }
    }

    pub(crate) fn run(&mut self, a: &mut [f64]) {
        let n = self.n;
        for (j, &(c, s)) in self.pre.iter().enumerate() {
            self.re[j] = a[j] * c;
            self.im[j] = a[j] * s;
        }
        self.re[n..].fill(0.0);
        self.im[n..].fill(0.0);
        self.fft.run(&mut self.re, &mut self.im);
        a[..n].copy_from_slice(&self.re[..n]);
    }
}

/// Inverse real DFT on Ooura's packed layout (`a[2j]` = Re X[j], `a[2j+1]` = Im X[j],
/// `a[1]` = Re X[n/2]):
/// `a[k] = (R0 + R(n/2) cos(pi k)) / 2 + sum_{0<j<n/2} R(j) cos(2 pi j k / n) + I(j) sin(2 pi j k / n)`.
pub(crate) struct Irdft {
    n: usize,
    fft: Fft,
    re: Vec<f64>,
    im: Vec<f64>,
}

impl Irdft {
    pub(crate) fn new(n: usize) -> Irdft {
        Irdft { n, fft: Fft::new(n), re: vec![0.0; n], im: vec![0.0; n] }
    }

    pub(crate) fn run(&mut self, a: &mut [f64]) {
        let n = self.n;
        self.re.fill(0.0);
        self.im.fill(0.0);
        self.re[0] = a[0] * 0.5;
        self.re[n / 2] = a[1] * 0.5;
        for j in 1..n / 2 {
            self.re[j] = a[2 * j];
            self.im[j] = -a[2 * j + 1];
        }
        self.fft.run(&mut self.re, &mut self.im);
        a[..n].copy_from_slice(&self.re[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dct3_matches_definition() {
        let n = 16;
        let src: Vec<f64> = (0..n).map(|i| ((i * 7 + 3) % 11) as f64 - 5.0).collect();
        let mut a = src.clone();
        Dct3::new(n).run(&mut a);
        for (k, &ak) in a.iter().enumerate() {
            let want: f64 = (0..n).map(|j| src[j] * (PI * j as f64 * (k as f64 + 0.5) / n as f64).cos()).sum();
            assert!((ak - want).abs() < 1e-9, "k {k}: {ak} vs {want}");
        }
    }

    #[test]
    fn irdft_matches_definition() {
        let n = 16;
        let src: Vec<f64> = (0..n).map(|i| ((i * 5 + 1) % 9) as f64 - 4.0).collect();
        let mut a = src.clone();
        Irdft::new(n).run(&mut a);
        for (k, &ak) in a.iter().enumerate() {
            let mut want = (src[0] + src[1] * (PI * k as f64).cos()) / 2.0;
            for j in 1..n / 2 {
                let t = 2.0 * PI * (j * k) as f64 / n as f64;
                want += src[2 * j] * t.cos() + src[2 * j + 1] * t.sin();
            }
            assert!((ak - want).abs() < 1e-9, "k {k}");
        }
    }
}

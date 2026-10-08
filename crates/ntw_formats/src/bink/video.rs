//! Bink 1 video decoding (revision `'i'`), our own implementation of the spec in
//! `analysis/video/BINK.md` §3–4.
//!
//! A frame is three planes (Y, then the two chroma planes at half size; an optional alpha plane
//! is not used by any shipped movie). Each plane is coded in rows of 8x8 blocks. At the start of
//! every block row the decoder refills nine "bundles" (block types, sub-block types, colours,
//! patterns, x and y motion offsets, intra and inter DC values, run lengths) from the
//! bitstream; the blocks of the row then take their values from the bundles and read their own
//! extra bits (DCT coefficients, residues, run flags) straight from the stream.
//!
//! Output matches the original decoder bit for bit as far as the spec goes: same integer IDCT,
//! same 8-bit wrap-around on adds (no clamping), same frame double-buffering.

use super::bits::BitReader;
use super::tables::{
    BLOCK_TYPE_RUNS, INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN, TREE_LOOKUP, TREE_MAX_BITS, TREE_OFFSETS,
};
use super::{BinkError, BinkHeader, FLAG_ALPHA, FLAG_GRAY};

/// A Huffman code set choice plus the 16-symbol permutation sent with it.
#[derive(Clone, Copy, Default)]
struct Tree {
    set: u8,
    syms: [u8; 16],
}

impl Tree {
    /// Reads a tree description (CONFIRMED against the DLL's reader, `BINK.md` §3.2).
    fn read(br: &mut BitReader) -> Tree {
        let set = br.read(4) as u8;
        let mut syms = [0u8; 16];
        if set == 0 {
            for (i, s) in syms.iter_mut().enumerate() {
                *s = i as u8;
            }
            return Tree { set, syms };
        }
        if br.bit() {
            // An explicit list of the first symbols, the rest follow in ascending order.
            let n = br.read(3) as usize;
            let mut used = 0u16;
            for s in syms.iter_mut().take(n + 1) {
                *s = br.read(4) as u8;
                used |= 1 << *s;
            }
            let mut k = n;
            for v in 0..16u8 {
                if k >= 15 {
                    break;
                }
                if used & (1 << v) == 0 {
                    k += 1;
                    syms[k] = v;
                }
            }
        } else {
            // Bit-driven merge sort of 0..16, 1 + depth passes.
            let depth = br.read(2);
            let mut a: [u8; 16] = std::array::from_fn(|i| i as u8);
            let mut b = [0u8; 16];
            for d in 0..=depth {
                let size = 1usize << d;
                let mut t = 0;
                while t < 16 {
                    merge(br, &a[t..t + size], &a[t + size..t + 2 * size], &mut b[t..t + 2 * size]);
                    t += 2 * size;
                }
                std::mem::swap(&mut a, &mut b);
            }
            syms = a;
        }
        Tree { set, syms }
    }

    #[inline]
    fn decode(&self, br: &mut BitReader) -> u8 {
        let bits = TREE_MAX_BITS[self.set as usize] as u32;
        let e = TREE_LOOKUP[TREE_OFFSETS[self.set as usize] as usize + br.peek(bits) as usize];
        br.skip((e >> 4) as u32);
        self.syms[(e & 15) as usize]
    }
}

fn merge(br: &mut BitReader, x: &[u8], y: &[u8], out: &mut [u8]) {
    let (mut i, mut j, mut o) = (0, 0, 0);
    while i < x.len() && j < y.len() {
        if br.bit() {
            out[o] = y[j];
            j += 1;
        } else {
            out[o] = x[i];
            i += 1;
        }
        o += 1;
    }
    while i < x.len() {
        out[o] = x[i];
        i += 1;
        o += 1;
    }
    while j < y.len() {
        out[o] = y[j];
        j += 1;
        o += 1;
    }
}

/// Number of bits of `v`'s highest set bit + 1 (the bundle count width rule).
fn bit_width(v: u32) -> u32 {
    32 - v.leading_zeros()
}

/// One bundle: a run of decoded values the blocks of the current rows take from.
struct Bundle<T: Copy + Default> {
    tree: Tree,
    data: Vec<T>,
    pos: usize,
    len: usize,
    /// A zero (or impossible) count was read: nothing more for this plane.
    done: bool,
    count_bits: u32,
    capacity: usize,
}

impl<T: Copy + Default> Bundle<T> {
    fn new(count_bits: u32, capacity: usize) -> Self {
        Bundle { tree: Tree::default(), data: vec![T::default(); capacity], pos: 0, len: 0, done: false, count_bits, capacity }
    }

    fn reset(&mut self) {
        self.pos = 0;
        self.len = 0;
        self.done = false;
    }

    /// Reads the value count if the previous values are used up. `None` = nothing to read now.
    fn begin(&mut self, br: &mut BitReader) -> Option<usize> {
        if self.done || self.pos < self.len {
            return None;
        }
        let n = br.read(self.count_bits) as usize;
        if n == 0 || n > self.capacity {
            self.done = true;
            self.pos = 0;
            self.len = 0;
            return None;
        }
        self.pos = 0;
        self.len = n;
        Some(n)
    }

    #[inline]
    fn next(&mut self) -> Result<T, &'static str> {
        if self.pos < self.len {
            let v = self.data[self.pos];
            self.pos += 1;
            Ok(v)
        } else {
            Err("bundle exhausted")
        }
    }

    fn has(&self) -> bool {
        self.pos < self.len
    }
}

/// All bundles of one plane.
struct Bundles {
    block_types: Bundle<u8>,
    sub_types: Bundle<u8>,
    colors: Bundle<u8>,
    color_high: [Tree; 16],
    color_last: u8,
    patterns: Bundle<u8>,
    x_off: Bundle<i8>,
    y_off: Bundle<i8>,
    intra_dc: Bundle<i16>,
    inter_dc: Bundle<i16>,
    runs: Bundle<u8>,
}

impl Bundles {
    /// Count widths from the plane width (CONFIRMED, `BINK.md` §3.1).
    fn new(width: u32) -> Self {
        let bw = width >> 3;
        let bits = |v: u32| bit_width(v + 511);
        let cap = |v: u32| (v + 512) as usize;
        Bundles {
            block_types: Bundle::new(bits(bw), cap(bw)),
            sub_types: Bundle::new(bits(width >> 4), cap(width >> 4)),
            colors: Bundle::new(bits(bw * 64), cap(bw * 64)),
            color_high: [Tree::default(); 16],
            color_last: 0,
            patterns: Bundle::new(bits(bw * 8), cap(bw * 8)),
            x_off: Bundle::new(bits(bw), cap(bw)),
            y_off: Bundle::new(bits(bw), cap(bw)),
            intra_dc: Bundle::new(bits(bw), cap(bw)),
            inter_dc: Bundle::new(bits(bw), cap(bw)),
            runs: Bundle::new(bits(bw * 48), cap(bw * 48)),
        }
    }

    /// Plane start: the trees, in stream order.
    fn read_trees(&mut self, br: &mut BitReader) {
        for b in [&mut self.block_types, &mut self.sub_types] {
            b.reset();
            b.tree = Tree::read(br);
        }
        for t in self.color_high.iter_mut() {
            *t = Tree::read(br);
        }
        self.colors.reset();
        self.colors.tree = Tree::read(br);
        self.color_last = 0;
        self.patterns.reset();
        self.patterns.tree = Tree::read(br);
        self.x_off.reset();
        self.x_off.tree = Tree::read(br);
        self.y_off.reset();
        self.y_off.tree = Tree::read(br);
        self.intra_dc.reset();
        self.inter_dc.reset();
        self.runs.reset();
        self.runs.tree = Tree::read(br);
    }

    /// Start of a block row: refill whichever bundles are used up, in stream order.
    fn refill(&mut self, br: &mut BitReader) {
        read_block_types(&mut self.block_types, br);
        read_block_types(&mut self.sub_types, br);
        self.read_colors(br);
        read_patterns(&mut self.patterns, br);
        read_motion(&mut self.x_off, br);
        read_motion(&mut self.y_off, br);
        read_dcs(&mut self.intra_dc, br, false);
        read_dcs(&mut self.inter_dc, br, true);
        read_runs(&mut self.runs, br);
    }

    fn read_colors(&mut self, br: &mut BitReader) {
        let Some(n) = self.colors.begin(br) else { return };
        let b = &mut self.colors;
        let fill = br.bit();
        let count = if fill { 1 } else { n };
        for i in 0..count {
            self.color_last = self.color_high[self.color_last as usize].decode(br);
            let lo = b.tree.decode(br);
            b.data[i] = self.color_last << 4 | lo;
        }
        if fill {
            let v = b.data[0];
            b.data[..n].fill(v);
        }
    }
}

fn read_block_types(b: &mut Bundle<u8>, br: &mut BitReader) {
    let Some(n) = b.begin(br) else { return };
    if br.bit() {
        let v = br.read(4) as u8;
        b.data[..n].fill(v);
        return;
    }
    let mut last = 0u8;
    let mut i = 0;
    while i < n {
        let v = b.tree.decode(br);
        if v < 12 {
            last = v;
            b.data[i] = v;
            i += 1;
        } else {
            let run = BLOCK_TYPE_RUNS[(v - 12) as usize] as usize;
            if n - i < run {
                // The original stops here (the rest of the bundle keeps stale values).
                b.len = i;
                return;
            }
            b.data[i..i + run].fill(last);
            i += run;
        }
    }
}

fn read_patterns(b: &mut Bundle<u8>, br: &mut BitReader) {
    let Some(n) = b.begin(br) else { return };
    for i in 0..n {
        let lo = b.tree.decode(br);
        let hi = b.tree.decode(br);
        b.data[i] = hi << 4 | lo;
    }
}

fn read_motion(b: &mut Bundle<i8>, br: &mut BitReader) {
    let Some(n) = b.begin(br) else { return };
    if br.bit() {
        let mut v = br.read(4) as i8;
        if v != 0 && br.bit() {
            v = -v;
        }
        b.data[..n].fill(v);
        return;
    }
    for i in 0..n {
        let mut v = b.tree.decode(br) as i8;
        if v != 0 && br.bit() {
            v = -v;
        }
        b.data[i] = v;
    }
}

fn read_runs(b: &mut Bundle<u8>, br: &mut BitReader) {
    let Some(n) = b.begin(br) else { return };
    if br.bit() {
        let v = br.read(4) as u8;
        b.data[..n].fill(v);
        return;
    }
    for i in 0..n {
        b.data[i] = b.tree.decode(br);
    }
}

/// DC values: an 11-bit start (signed for inter blocks), then groups of up to 8 deltas.
fn read_dcs(b: &mut Bundle<i16>, br: &mut BitReader, signed: bool) {
    let Some(n) = b.begin(br) else { return };
    let mut v: i32 = if signed {
        let m = br.read(10) as i32;
        if m != 0 && br.bit() { -m } else { m }
    } else {
        br.read(11) as i32
    };
    b.data[0] = v as i16;
    let mut i = 1;
    while i < n {
        let group = (n - i).min(8);
        let size = br.read(4);
        for k in 0..group {
            if size != 0 {
                let d = br.read(size) as i32;
                if d != 0 {
                    v = v.wrapping_add(if br.bit() { -d } else { d });
                }
            }
            b.data[i + k] = v as i16;
        }
        i += group;
    }
}

/// Reads the AC coefficients of a DCT block (stream order, then stored at natural positions).
/// `block[0]` (the DC) is set by the caller. CONFIRMED against the DLL's reader (`BINK.md` §3.4).
fn read_dct_coeffs(br: &mut BitReader, block: &mut [i16; 64]) {
    // Work list entries: coefficient index << 2 | mode. Mode-3 entries are pushed at the front.
    let mut list = [0u8; 128];
    let mut start = 64usize;
    let mut end = 64usize;
    for e in [4 << 2, 24 << 2, 44 << 2, 1 << 2 | 3, 2 << 2 | 3, 3 << 2 | 3] {
        list[end] = e;
        end += 1;
    }
    let nbits = br.read(4);
    // Passes with `bits` = nbits-1 .. 0 magnitude bits.
    for bits in (0..nbits).rev() {
        let mut pos = start;
        while pos < end {
            let e = list[pos];
            if e == 0 || !br.bit() {
                pos += 1;
                continue;
            }
            let coef = (e >> 2) as usize;
            match e & 3 {
                0 | 2 => {
                    if e & 3 == 0 {
                        list[pos] = ((coef + 4) as u8) << 2 | 1;
                    } else {
                        list[pos] = 0;
                        pos += 1;
                    }
                    for c in coef..coef + 4 {
                        if br.bit() {
                            start -= 1;
                            list[start] = (c as u8) << 2 | 3;
                        } else {
                            block[SCAN[c] as usize] = read_coef(br, bits);
                        }
                    }
                }
                1 => {
                    list[pos] = (coef as u8) << 2 | 2;
                    for k in 1..=3 {
                        list[end] = ((coef + 4 * k) as u8) << 2 | 2;
                        end += 1;
                    }
                }
                _ => {
                    block[SCAN[coef] as usize] = read_coef(br, bits);
                    list[pos] = 0;
                    pos += 1;
                }
            }
        }
    }
}

#[inline]
fn read_coef(br: &mut BitReader, bits: u32) -> i16 {
    if bits == 0 {
        if br.bit() { -1 } else { 1 }
    } else {
        let v = (br.read(bits) | 1 << bits) as i16;
        if br.bit() { v.wrapping_neg() } else { v }
    }
}

/// Reads a residue block (motion + residue blocks): values in stream order, bit-plane coded,
/// stopping after `masks + 1` mask applications. CONFIRMED against the DLL (`BINK.md` §3.5).
fn read_residue(br: &mut BitReader, block: &mut [i8; 64], masks: u32) {
    let mut list = [0u8; 128];
    let mut start = 64usize;
    let mut end = 64usize;
    for e in [4 << 2, 24 << 2, 44 << 2, 2] {
        list[end] = e;
        end += 1;
    }
    let mut nz = [0u8; 64];
    let mut nz_count = 0usize;
    let mut applied: u32 = 0;
    let top = br.read(3);
    let mut mask: i32 = 1 << top;
    for _ in 0..=top {
        // Refine the coefficients found so far.
        for &c in &nz[..nz_count] {
            if br.bit() {
                let v = block[c as usize];
                let m = mask as i8;
                block[c as usize] = if v < 0 { v.wrapping_sub(m) } else { v.wrapping_add(m) };
                if applied == masks {
                    return;
                }
                applied += 1;
            }
        }
        let mut pos = start;
        while pos < end {
            let e = list[pos];
            if e == 0 || !br.bit() {
                pos += 1;
                continue;
            }
            let coef = (e >> 2) as usize;
            match e & 3 {
                0 | 2 => {
                    if e & 3 == 0 {
                        list[pos] = ((coef + 4) as u8) << 2 | 1;
                    } else {
                        list[pos] = 0;
                        pos += 1;
                    }
                    #[allow(clippy::needless_range_loop)] // `c` is also the coded value; mirrors the DLL loop
                    for c in coef..coef + 4 {
                        if br.bit() {
                            start -= 1;
                            list[start] = (c as u8) << 2 | 3;
                        } else {
                            nz[nz_count] = c as u8;
                            nz_count += 1;
                            block[c] = if br.bit() { (-mask) as i8 } else { mask as i8 };
                            if applied == masks {
                                return;
                            }
                            applied += 1;
                        }
                    }
                }
                1 => {
                    list[pos] = (coef as u8) << 2 | 2;
                    for k in 1..=3 {
                        list[end] = ((coef + 4 * k) as u8) << 2 | 2;
                        end += 1;
                    }
                }
                _ => {
                    nz[nz_count] = coef as u8;
                    nz_count += 1;
                    block[coef] = if br.bit() { (-mask) as i8 } else { mask as i8 };
                    list[pos] = 0;
                    pos += 1;
                    if applied == masks {
                        return;
                    }
                    applied += 1;
                }
            }
        }
        mask >>= 1;
    }
}

/// The integer 8x8 inverse DCT with dequantisation folded into the column pass. Returns the
/// row-pass outputs before the final `(x + 127) >> 8`. CONFIRMED constants and order.
fn idct(block: &[i16; 64], quant: &[u32; 64]) -> [i32; 64] {
    const A1: i32 = 2896;
    const A2: i32 = 2217;
    const A3: i32 = 3784;
    const A4: i32 = -5352;
    let m = |x: i32, c: i32| x.wrapping_mul(c) >> 11;
    let mut t = [0i32; 64];
    for i in 0..8 {
        let s = |r: usize| (block[r * 8 + i] as i32).wrapping_mul(quant[r * 8 + i] as i32) >> 11;
        if (1..8).all(|r| block[r * 8 + i] == 0) {
            let v = s(0);
            for r in 0..8 {
                t[r * 8 + i] = v;
            }
            continue;
        }
        let o = butterfly([s(0), s(1), s(2), s(3), s(4), s(5), s(6), s(7)], A1, A2, A3, A4, &m);
        for r in 0..8 {
            t[r * 8 + i] = o[r];
        }
    }
    let mut out = [0i32; 64];
    for r in 0..8 {
        let row: [i32; 8] = t[r * 8..r * 8 + 8].try_into().unwrap();
        let o = butterfly(row, A1, A2, A3, A4, &m);
        out[r * 8..r * 8 + 8].copy_from_slice(&o);
    }
    out
}

#[inline]
fn butterfly(s: [i32; 8], a1: i32, a2: i32, a3: i32, a4: i32, m: &impl Fn(i32, i32) -> i32) -> [i32; 8] {
    let a0 = s[0].wrapping_add(s[4]);
    let a1_ = s[0].wrapping_sub(s[4]);
    let a2_ = s[2].wrapping_add(s[6]);
    let a3_ = m(s[2].wrapping_sub(s[6]), a1);
    let a4_ = s[5].wrapping_add(s[3]);
    let a5 = s[5].wrapping_sub(s[3]);
    let a6 = s[1].wrapping_add(s[7]);
    let a7 = s[1].wrapping_sub(s[7]);
    let b0 = a4_.wrapping_add(a6);
    let b1 = m(a5.wrapping_add(a7), a3);
    let b2 = m(a5, a4).wrapping_sub(b0).wrapping_add(b1);
    let b3 = m(a6.wrapping_sub(a4_), a1).wrapping_sub(b2);
    let b4 = m(a7, a2).wrapping_add(b3).wrapping_sub(b1);
    let e0 = a0.wrapping_add(a2_);
    let e1 = a1_.wrapping_add(a3_).wrapping_sub(a2_);
    let e2 = a1_.wrapping_sub(a3_).wrapping_add(a2_);
    let e3 = a0.wrapping_sub(a2_);
    [
        e0.wrapping_add(b0),
        e1.wrapping_add(b2),
        e2.wrapping_add(b3),
        e3.wrapping_sub(b4),
        e3.wrapping_add(b4),
        e2.wrapping_sub(b3),
        e1.wrapping_sub(b2),
        e0.wrapping_sub(b0),
    ]
}

#[inline]
fn px(v: i32) -> u8 {
    (v.wrapping_add(0x7f) >> 8) as u8
}

/// One decoded picture: Y at full size, the two chroma planes at half size, all padded to
/// multiples of 8 (the buffer sizes of the original). `width`/`height` are the visible size.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Padded plane sizes: `(y_w, y_h)` and `(c_w, c_h)`; the strides equal the widths.
    pub y_size: (u32, u32),
    pub c_size: (u32, u32),
    pub y: Vec<u8>,
    /// Cb (U).
    pub u: Vec<u8>,
    /// Cr (V).
    pub v: Vec<u8>,
}

impl Frame {
    fn new(width: u32, height: u32) -> Self {
        // Buffer sizes CONFIRMED from BinkGetFrameBuffersInfo: Y (w+7)&~7 x (h+7)&~7,
        // chroma (((w+1)>>1)+7)&~7 x (((h+1)>>1)+7)&~7.
        let yw = (width + 7) & !7;
        let yh = (height + 7) & !7;
        let cw = (((width + 1) >> 1) + 7) & !7;
        let ch = (((height + 1) >> 1) + 7) & !7;
        Frame {
            width,
            height,
            y_size: (yw, yh),
            c_size: (cw, ch),
            y: vec![0; (yw * yh) as usize],
            u: vec![0; (cw * ch) as usize],
            v: vec![0; (cw * ch) as usize],
        }
    }

    /// Converts to RGBA8 (`width * height * 4` bytes) the way the game shows movies, with the
    /// default gamma and brightness settings ([`YuvToRgb::default`]).
    pub fn to_rgba(&self, out: &mut [u8]) {
        self.to_rgba_with(out, &YuvToRgb::default());
    }

    /// Converts to RGBA8 with the original's movie shader maths (`fx\sprite.fx`, `pixel_yuv`,
    /// technique `normal_yuv_t0`; `BINK.md` §6): the planes are separate 8-bit textures sampled
    /// with bilinear filtering, so chroma is upsampled with 3/4 + 1/4 weights (INFERRED: texel
    /// centres aligned; the D3D9 half-pixel handling of the quad is not checked), then
    /// `rgb = 1.164123535 Y + crc Cr + crb Cb + adj` in 0..1 units, `pow(abs(rgb), 2 / gamma)`,
    /// times `brightness / 1.2`, clamped and rounded to 8 bits by the render target.
    pub fn to_rgba_with(&self, out: &mut [u8], conv: &YuvToRgb) {
        let (w, h) = (self.width as usize, self.height as usize);
        let ys = self.y_size.0 as usize;
        let (cw, ch) = (self.c_size.0 as usize, self.c_size.1 as usize);
        assert!(out.len() >= w * h * 4);
        let exponent = 2.0 / conv.gamma;
        let gain = conv.brightness / 1.2;
        let plain = exponent == 1.0;
        // Bilinear chroma taps for one output coordinate: (lower texel, upper texel, upper weight).
        let taps = |i: usize, n: usize| -> (usize, usize, f32) {
            let c = i as f32 * 0.5 - 0.25;
            let lo = c.floor();
            let f = c - lo;
            let lo = lo as isize;
            let clamp = |v: isize| v.clamp(0, n as isize - 1) as usize;
            (clamp(lo), clamp(lo + 1), f)
        };
        let xt: Vec<(usize, usize, f32)> = (0..w).map(|x| taps(x, cw)).collect();
        let mut cb_row = vec![0f32; cw];
        let mut cr_row = vec![0f32; cw];
        for yy in 0..h {
            let (r0, r1, fy) = taps(yy, ch);
            for c in 0..cw {
                let lerp = |p: &[u8]| p[r0 * cw + c] as f32 * (1.0 - fy) + p[r1 * cw + c] as f32 * fy;
                cb_row[c] = lerp(&self.u) / 255.0;
                cr_row[c] = lerp(&self.v) / 255.0;
            }
            let orow = &mut out[yy * w * 4..(yy + 1) * w * 4];
            for (xx, &(c0, c1, fx)) in xt.iter().enumerate() {
                let y = self.y[yy * ys + xx] as f32 / 255.0;
                let cb = cb_row[c0] * (1.0 - fx) + cb_row[c1] * fx;
                let cr = cr_row[c0] * (1.0 - fx) + cr_row[c1] * fx;
                let yl = y * 1.164_123_5;
                let mut rgb = [
                    yl + 1.595_794_7 * cr - 0.870_655_06,
                    yl - 0.813_476_56 * cr - 0.391_448_98 * cb + 0.529_705_05,
                    yl + 2.017_822_3 * cb - 1.081_668_9,
                ];
                for v in rgb.iter_mut() {
                    if !plain {
                        *v = v.abs().powf(exponent);
                    }
                    *v *= gain;
                }
                let o = &mut orow[xx * 4..xx * 4 + 4];
                for k in 0..3 {
                    o[k] = (rgb[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                o[3] = 255;
            }
        }
    }
}

/// The user settings the movie shader uses: `gfx_gamma_setting` (default 2, which makes the
/// gamma step a no-op) and `gfx_brightness_setting` (default 1.2, cancelled by the shader's
/// `1/1.2`). CONFIRMED: `GAMMA_VALUE` is set from the renderer's gamma at shader compile time
/// (Napoleon.exe `0x011A7F10`); defaults from the preferences file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YuvToRgb {
    pub gamma: f32,
    pub brightness: f32,
}

impl Default for YuvToRgb {
    fn default() -> Self {
        YuvToRgb { gamma: 2.0, brightness: 1.2 }
    }
}

/// Decodes the video frames of one movie, in order.
pub struct VideoDecoder {
    frames: [Frame; 2],
    /// Index of the frame holding the latest picture.
    cur: usize,
    flags: u32,
    decoded: u32,
}

/// Where each plane's data ended, in bits from the start of the video packet (for checks).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaneEnds {
    /// Bit position after the Y plane (before 32-bit alignment).
    pub y_end: usize,
    /// Byte offset where the chroma data starts, as stored at the start of the packet.
    pub chroma_offset: usize,
    /// Bit position after the first and second chroma planes.
    pub c1_end: usize,
    pub c2_end: usize,
}

impl VideoDecoder {
    pub fn new(header: &BinkHeader) -> Result<Self, BinkError> {
        if header.revision < b'h' {
            // Older revisions have no plane offset word and other colour coding.
            return Err(BinkError::UnsupportedRevision(header.revision));
        }
        if header.flags & FLAG_ALPHA != 0 {
            // No shipped movie has alpha; the plane would decode like Y but is not implemented.
            return Err(BinkError::BadHeader("alpha plane not supported"));
        }
        Ok(VideoDecoder {
            frames: [Frame::new(header.width, header.height), Frame::new(header.width, header.height)],
            cur: 0,
            flags: header.flags,
            decoded: 0,
        })
    }

    /// The latest decoded picture.
    pub fn frame(&self) -> &Frame {
        &self.frames[self.cur]
    }

    /// Decodes the next frame's video packet (from [`super::split_packet`]).
    pub fn decode(&mut self, video: &[u8]) -> Result<PlaneEnds, BinkError> {
        let frame_no = self.decoded;
        let bad = |what: String| BinkError::BadFrame { frame: frame_no, what };
        let next = self.cur ^ 1;
        let (a, b) = self.frames.split_at_mut(1);
        let (dst, src) = if next == 0 { (&mut a[0], &b[0]) } else { (&mut b[0], &a[0]) };
        if video.len() < 4 {
            return Err(bad("video packet shorter than 4 bytes".into()));
        }
        let chroma_offset = u32::from_le_bytes(video[..4].try_into().unwrap()) as usize;
        let mut ends = PlaneEnds { chroma_offset, ..Default::default() };
        let mut br = BitReader::new(video);
        br.seek(32);
        let (yw, yh) = dst.y_size;
        decode_plane(&mut br, &mut dst.y, &src.y, yw, yh).map_err(|e| bad(format!("Y plane: {e}")))?;
        ends.y_end = br.position();
        if self.flags & FLAG_GRAY == 0 {
            if chroma_offset < 4 || chroma_offset > video.len() {
                return Err(bad(format!("chroma offset {chroma_offset} outside packet of {}", video.len())));
            }
            let (cw, ch) = dst.c_size;
            br.seek(chroma_offset * 8);
            // First the Cr plane, then Cb (CONFIRMED plane order of BINKFRAMEPLANESET use).
            decode_plane(&mut br, &mut dst.v, &src.v, cw, ch).map_err(|e| bad(format!("Cr plane: {e}")))?;
            ends.c1_end = br.position();
            br.align32();
            decode_plane(&mut br, &mut dst.u, &src.u, cw, ch).map_err(|e| bad(format!("Cb plane: {e}")))?;
            ends.c2_end = br.position();
        }
        if br.overrun() {
            return Err(bad("read past the end of the packet".into()));
        }
        self.cur = next;
        self.decoded += 1;
        Ok(ends)
    }
}

fn copy8(dst: &mut [u8], d: usize, src: &[u8], s: usize, stride: usize) {
    for r in 0..8 {
        dst[d + r * stride..d + r * stride + 8].copy_from_slice(&src[s + r * stride..s + r * stride + 8]);
    }
}

fn put8(dst: &mut [u8], d: usize, stride: usize, block: &[u8; 64]) {
    for r in 0..8 {
        dst[d + r * stride..d + r * stride + 8].copy_from_slice(&block[r * 8..r * 8 + 8]);
    }
}

fn put_scaled(dst: &mut [u8], d: usize, stride: usize, block: &[u8; 64]) {
    for r in 0..8 {
        for c in 0..8 {
            let v = block[r * 8 + c];
            let o = d + 2 * r * stride + 2 * c;
            dst[o] = v;
            dst[o + 1] = v;
            dst[o + stride] = v;
            dst[o + stride + 1] = v;
        }
    }
}

/// Run block: a pattern order, then runs of single colours or of individual colours.
fn run_block(br: &mut BitReader, bd: &mut Bundles, out: &mut [u8; 64]) -> Result<(), &'static str> {
    let scan = &PATTERNS[br.read(4) as usize * 64..][..64];
    let mut i = 0usize;
    let mut left = 64usize;
    loop {
        let run = bd.runs.next()? as usize + 1;
        if run > left {
            break;
        }
        left -= run;
        if br.bit() {
            let c = bd.colors.next()?;
            for _ in 0..run {
                out[scan[i] as usize] = c;
                i += 1;
            }
        } else {
            for _ in 0..run {
                out[scan[i] as usize] = bd.colors.next()?;
                i += 1;
            }
        }
        if left <= 1 {
            break;
        }
    }
    if i == 63 {
        out[scan[63] as usize] = bd.colors.next()?;
    }
    Ok(())
}

fn pattern_block(bd: &mut Bundles, out: &mut [u8; 64]) -> Result<(), &'static str> {
    let c0 = bd.colors.next()?;
    let c1 = bd.colors.next()?;
    for r in 0..8 {
        let p = bd.patterns.next()?;
        for c in 0..8 {
            out[r * 8 + c] = if p >> c & 1 != 0 { c1 } else { c0 };
        }
    }
    Ok(())
}

fn intra_block(br: &mut BitReader, bd: &mut Bundles, out: &mut [u8; 64]) -> Result<(), &'static str> {
    let mut block = [0i16; 64];
    block[0] = bd.intra_dc.next()?;
    read_dct_coeffs(br, &mut block);
    let q = br.read(4) as usize;
    let v = idct(&block, &INTRA_QUANT[q]);
    for (o, x) in out.iter_mut().zip(v) {
        *o = px(x);
    }
    Ok(())
}

/// Decodes one plane into `dst` (`w` x `h`, stride `w`), with `src` the previous frame's plane.
fn decode_plane(br: &mut BitReader, dst: &mut [u8], src: &[u8], w: u32, h: u32) -> Result<(), &'static str> {
    let stride = w as usize;
    let (w, h) = (w as usize, h as usize);
    let mut bd = Bundles::new(w as u32);
    bd.read_trees(br);
    // The last position a motion source block may start at (a linear check, as the original).
    let src_max = (h - 8) * stride + w - 8;
    let mut ublock = [0u8; 64];
    let mut by = 0usize;
    while by < h {
        bd.refill(br);
        let mut x = 0usize;
        while x < w {
            let d = by * stride + x;
            let bt = bd.block_types.next()?;
            let mut step = 8;
            match bt {
                0 => copy8(dst, d, src, d, stride),
                1 => {
                    if w - x < 16 {
                        // Not enough room: treated as an 8-pixel no-op, sub-type not read.
                    } else if by & 8 != 0 {
                        // Odd row: covered by the scaled block above.
                        step = 16;
                    } else if !bd.sub_types.has() {
                        step = 16;
                    } else {
                        let st = bd.sub_types.next()?;
                        if h - by < 16 {
                            // No room below: an 8-pixel no-op (sub-type consumed).
                        } else {
                            step = 16;
                            match st {
                                3 => {
                                    run_block(br, &mut bd, &mut ublock)?;
                                    put_scaled(dst, d, stride, &ublock);
                                }
                                5 => {
                                    intra_block(br, &mut bd, &mut ublock)?;
                                    put_scaled(dst, d, stride, &ublock);
                                }
                                6 => {
                                    let c = bd.colors.next()?;
                                    for r in 0..16 {
                                        dst[d + r * stride..d + r * stride + 16].fill(c);
                                    }
                                }
                                8 => {
                                    pattern_block(&mut bd, &mut ublock)?;
                                    put_scaled(dst, d, stride, &ublock);
                                }
                                9 => {
                                    for p in ublock.iter_mut() {
                                        *p = bd.colors.next()?;
                                    }
                                    put_scaled(dst, d, stride, &ublock);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                2 | 4 | 7 => {
                    let xo = bd.x_off.next()? as isize;
                    let yo = bd.y_off.next()? as isize;
                    let s = d as isize + xo + yo * stride as isize;
                    if s >= 0 && s as usize <= src_max {
                        let s = s as usize;
                        match bt {
                            2 => copy8(dst, d, src, s, stride),
                            4 => {
                                let masks = br.read(7);
                                let mut res = [0i8; 64];
                                read_residue(br, &mut res, masks);
                                for r in 0..8 {
                                    for c in 0..8 {
                                        dst[d + r * stride + c] = src[s + r * stride + c];
                                    }
                                }
                                for (k, &v) in res.iter().enumerate() {
                                    let n = SCAN[k] as usize;
                                    let o = d + (n >> 3) * stride + (n & 7);
                                    dst[o] = dst[o].wrapping_add(v as u8);
                                }
                            }
                            _ => {
                                let mut block = [0i16; 64];
                                block[0] = bd.inter_dc.next()?;
                                read_dct_coeffs(br, &mut block);
                                let q = br.read(4) as usize;
                                let v = idct(&block, &INTER_QUANT[q]);
                                for r in 0..8 {
                                    for c in 0..8 {
                                        dst[d + r * stride + c] =
                                            src[s + r * stride + c].wrapping_add(px(v[r * 8 + c]));
                                    }
                                }
                            }
                        }
                    }
                }
                3 => {
                    run_block(br, &mut bd, &mut ublock)?;
                    put8(dst, d, stride, &ublock);
                }
                5 => {
                    intra_block(br, &mut bd, &mut ublock)?;
                    put8(dst, d, stride, &ublock);
                }
                6 => {
                    let c = bd.colors.next()?;
                    for r in 0..8 {
                        dst[d + r * stride..d + r * stride + 8].fill(c);
                    }
                }
                8 => {
                    pattern_block(&mut bd, &mut ublock)?;
                    put8(dst, d, stride, &ublock);
                }
                9 => {
                    for p in ublock.iter_mut() {
                        *p = bd.colors.next()?;
                    }
                    put8(dst, d, stride, &ublock);
                }
                _ => {}
            }
            x += step;
        }
        by += 8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every code set's lookup table is a complete prefix code over 16 slots.
    #[test]
    fn tree_tables_are_prefix_codes() {
        for set in 0..16 {
            let bits = TREE_MAX_BITS[set] as usize;
            let t = &TREE_LOOKUP[TREE_OFFSETS[set] as usize..][..1 << bits];
            let mut len_of = [0usize; 16];
            for (idx, &e) in t.iter().enumerate() {
                let len = (e >> 4) as usize;
                assert!(len >= 1 && len <= bits, "set {set}");
                // Every index sharing the low `len` bits must give the same entry.
                assert_eq!(t[idx & ((1 << len) - 1)], e, "set {set} idx {idx}");
                len_of[(e & 15) as usize] = len;
            }
            // Kraft sum of the 16 codes is exactly 1: a complete prefix code.
            assert!(len_of.iter().all(|&l| l > 0), "set {set} misses a symbol");
            let total: usize = len_of.iter().map(|&l| 1usize << (bits - l)).sum();
            assert_eq!(total, 1 << bits, "set {set}");
        }
    }

    #[test]
    fn idct_of_dc_is_flat() {
        let mut b = [0i16; 64];
        b[0] = 64;
        let q = [2048u32; 64];
        let v = idct(&b, &q);
        // 64 * 2048 >> 11 = 64 per column, then 8x scaling in rows... flat in any case.
        assert!(v.iter().all(|&x| x == v[0]));
    }

    #[test]
    fn scan_and_patterns_are_permutations() {
        let mut s = SCAN.to_vec();
        s.sort();
        assert_eq!(s, (0..64).collect::<Vec<u8>>());
        for p in 0..16 {
            let mut v = PATTERNS[p * 64..p * 64 + 64].to_vec();
            v.sort();
            assert_eq!(v, (0..64).collect::<Vec<u8>>());
        }
    }
}

//! Research helper for `.cuf` fonts. Read-only.
//!   cargo run -p ntw_formats --release --example cuf_probe -- <font path> [chars]
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let b = vfs.read(&args[0]).unwrap();
    let hdr: Vec<i16> = (0..12).map(|i| u16_at(&b, 4 + 2 * i) as i16).collect();
    let n = hdr[11] as usize;
    let pix = u32::from_le_bytes(b[28..32].try_into().unwrap()) as usize;
    let rec = 0x20020;
    let offs = rec + 4 * n;
    let pixo = offs + 4 * n;
    let kern = pixo + pix;
    let kn = u16_at(&b, kern) as usize;
    let kf = u16_at(&b, kern + 2) as usize;
    println!("len {} hdr {:?} n {} pix {} kern@{:x} count {} first {} expected_end {}", b.len(), hdr, n, pix, kern, kn, kf, kern + 4 + kn * kn);
    let chars = args.get(1).cloned().unwrap_or_else(|| "!\"AVWaio.".into());
    for ch in chars.chars() {
        let c = ch as usize;
        let g = u16_at(&b, 0x20 + 2 * c) as usize;
        let r = &b[rec + 4 * g..rec + 4 * g + 4];
        let row: Vec<u8> = if (kf..kf + kn).contains(&c) {
            let s = kern + 4 + (c - kf) * kn;
            ["A", "V", "a", "o", " ", "."].iter().map(|x| b[s + (x.chars().next().unwrap() as usize - kf).min(kn - 1)]).collect()
        } else {
            vec![]
        };
        println!("{ch:?} glyph {g} rec {:?} pair(A,V,a,o,sp,.) {:?}", r, row);
    }
}

//! Research helper: decode a DDS from the packs to a PNG (scratch output only).
//! `cargo run -p ntw_formats --example dds_png -- <pack path or file> <out.png>` (DDS or TGA)
use ntw_formats::dds::Dds;
use ntw_formats::pack::Vfs;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data").unwrap();
    // A loose file path also works, and TGA pictures (e.g. the campaign maps' radar pictures).
    let b = std::fs::read(&a[0]).or_else(|_| vfs.read(&a[0])).unwrap();
    let (w, h, px) = if a[0].to_ascii_lowercase().ends_with(".tga") {
        let t = ntw_formats::tga::Tga::decode(&b).unwrap();
        (t.width, t.height, t.rgba)
    } else {
        let d = Dds::parse(&b).unwrap();
        println!("{:?} {}x{}", d.format, d.width, d.height);
        (d.width, d.height, d.decode_rgba8(0))
    };
    let mut raw = Vec::new();
    for y in 0..h as usize {
        raw.push(0u8);
        raw.extend_from_slice(&px[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    let z = miniz_oxide_compress(&raw);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let crc = |d: &[u8]| { let mut c = 0xFFFF_FFFFu32; for &x in d { c ^= x as u32; for _ in 0..8 { c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }; } } !c };
    let mut chunk = |t: &[u8], d: &[u8]| { out.extend_from_slice(&(d.len() as u32).to_be_bytes()); let mut td = t.to_vec(); td.extend_from_slice(d); out.extend_from_slice(&td); out.extend_from_slice(&crc(&td).to_be_bytes()); };
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(&a[1], out).unwrap();
}

/// Stored-block zlib stream.
fn miniz_oxide_compress(raw: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    for (i, c) in raw.chunks(65535).enumerate() {
        z.push(((i + 1) * 65535 >= raw.len()) as u8);
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in raw { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    z
}

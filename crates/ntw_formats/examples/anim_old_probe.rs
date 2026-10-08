//! Research probe: the 31 older `testdata\animations` clips (read-only). Prints, per file, the
//! bytes left after the skeleton and which key sizes divide them.
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    for p in vfs.list("testdata/animations") {
        let b = vfs.read(p).unwrap();
        let rd = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let fr = f32::from_le_bytes(b[0..4].try_into().unwrap());
        let du = f32::from_le_bytes(b[4..8].try_into().unwrap());
        let n = rd(8) as usize;
        let mut o = 12;
        for _ in 0..n {
            let l = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
            o += 2 + 2 * l + 4;
        }
        let frames = rd(o) as usize;
        let rest = b.len() - o - 4;
        let per = rest as f64 / (frames.max(1) * n) as f64;
        let ok = ntw_formats::anim::Anim::read(&b).map(|_| ()).map_err(|e| e.to_string());
        if ok.is_ok() { continue; }
        println!("{ok:?} {p}: rate {fr} dur {du} bones {n} frames {frames} rest {rest} bytes/key {per:.3} head {:02x?}", &b[o + 4..o + 4 + 32]);
    }
}

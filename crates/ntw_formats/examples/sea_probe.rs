//! Research helper: the engine's own texture container (`magic 0x12345678`, version 1) as the sea
//! and wind textures use it (`sea\sea`, `sea\swell`, `wind_level_<n>_sea` — no extension). The
//! renderer needs to know the header and the block layout. Read-only.
//!
//!   cargo run -p ntw_formats --example sea_probe -- header <vfs path>
//!   cargo run -p ntw_formats --example sea_probe -- chunks <vfs path> [header_len]
//!
//! `chunks` walks the "u32 byte count, then that many bytes" chain that the first level sizes
//! suggest and prints where it lands, so a format can be confirmed against the file length.
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let bytes = vfs.read(&args[1]).unwrap();
    match args[0].as_str() {
        "header" => {
            println!("len {}", bytes.len());
            for i in 0..bytes.len() / 4 {
                let u = u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
                let f = f32::from_bits(u);
                println!("{i:>4} 0x{i:04x}  u32 {u:>12}  f32 {f:>16.6}  bytes {:02x?}", &bytes[i * 4..i * 4 + 4]);
                if i > 31 {
                    break;
                }
            }
        }
        "chunks" => {
            // header: first bytes printed; chunk chain from a given offset.
            let start: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(20);
            let mut at = start;
            for level in 0..24 {
                if at + 4 > bytes.len() {
                    println!("level {level}: past end at {at}");
                    return;
                }
                let n = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
                let next = at + 4 + n;
                println!("level {level}: at 0x{at:x} size {n} -> next 0x{next:x} ({:.1}% of file)",
                    100.0 * next as f64 / bytes.len() as f64);
                at = next;
            }
        }
        _ => eprintln!("unknown command"),
    }
}

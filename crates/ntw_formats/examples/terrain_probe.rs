//! Research probe for battle terrain files (read-only, prints to stdout, writes nothing).
//!
//! ```text
//! cargo run -p ntw_formats --example terrain_probe -- ls <prefix> [max]
//! cargo run -p ntw_formats --example terrain_probe -- cat <path> [max_bytes]
//! cargo run -p ntw_formats --example terrain_probe -- hex <path> [n] [skip]
//! ```
use ntw_formats::pack::Vfs;

const DEFAULT_DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = std::env::var("NAPOLEON_INSTALL_DIR")
        .map(|d| format!(r"{d}\data"))
        .unwrap_or_else(|_| DEFAULT_DATA.to_string());
    let vfs = Vfs::open_install(&data).expect("open install");
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let num = |i: usize, d: usize| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    match arg(0).as_str() {
        "ls" => {
            let all = vfs.list(&arg(1));
            for p in all.iter().take(num(2, 200)) {
                let size = vfs.find(p).map(|(_, e)| e.size).unwrap_or(0);
                println!("{size:10} {p}");
            }
            println!("({} total)", all.len());
        }
        "cat" => {
            let b = vfs.read(&arg(1)).expect("read");
            let n = num(2, 4000).min(b.len());
            println!("{}", String::from_utf8_lossy(&b[..n]));
        }
        "hex" => {
            let b = vfs.read(&arg(1)).expect("read");
            let skip = num(3, 0);
            let end = (skip + num(2, 256)).min(b.len());
            for (i, row) in b[skip..end].chunks(16).enumerate() {
                let hex: Vec<String> = row.iter().map(|x| format!("{x:02x}")).collect();
                let txt: String =
                    row.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect();
                println!("{:08x}  {:48} {txt}", skip + i * 16, hex.join(" "));
            }
            println!("(file is {} bytes)", b.len());
        }
        "grep" => {
            for p in vfs.list(&arg(1)) {
                let b = vfs.read(p).unwrap_or_default();
                let needle = arg(2).into_bytes();
                let hits = b.windows(needle.len()).filter(|w| *w == &needle[..]).count();
                if hits > 0 {
                    println!("{hits:6} {p}");
                }
            }
        }
        _ => eprintln!("usage: terrain_probe ls|cat|hex|grep ..."),
    }
}

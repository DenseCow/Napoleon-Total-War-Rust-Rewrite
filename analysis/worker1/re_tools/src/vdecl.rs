//! Find D3DVERTEXELEMENT9 arrays (8-byte elements ending with D3DDECL_END {0xFF,0,17,0,0,0}) in non-code sections.
use crate::pe::Pe;
use std::fmt::Write as _;

const TYPES: [&str; 18] = ["FLOAT1","FLOAT2","FLOAT3","FLOAT4","D3DCOLOR","UBYTE4","SHORT2","SHORT4","UBYTE4N","SHORT2N","SHORT4N","USHORT2N","USHORT4N","UDEC3","DEC3N","FLOAT16_2","FLOAT16_4","UNUSED"];
const USAGES: [&str; 14] = ["POSITION","BLENDWEIGHT","BLENDINDICES","NORMAL","PSIZE","TEXCOORD","TANGENT","BINORMAL","TESSFACTOR","POSITIONT","COLOR","FOG","DEPTH","SAMPLE"];
const SIZES: [u32; 17] = [4,8,12,16,4,4,4,8,4,4,8,4,8,4,4,4,8];

pub fn run(p: &Pe, out: &str) {
    let d = &p.d;
    let mut w = String::new();
    let mut n = 0;
    for s in &p.sections {
        if s.ch & 0x20000000 != 0 || s.name == ".reloc" || s.name == ".rsrc" { continue; }
        let (a, b) = (s.raw as usize, (s.raw + s.rsize) as usize);
        let mut i = a;
        while i + 8 <= b {
            if d[i..i + 8] == [0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00] {
                // walk back over valid elements
                let mut els = vec![];
                let mut j = i;
                while j >= a + 8 {
                    let e = &d[j - 8..j];
                    let stream = u16::from_le_bytes([e[0], e[1]]);
                    let off = u16::from_le_bytes([e[2], e[3]]);
                    let (ty, meth, usage, ui) = (e[4], e[5], e[6], e[7]);
                    if stream > 15 || off > 256 || ty > 16 || meth > 7 || usage > 13 || ui > 15 { break; }
                    els.push((stream, off, ty, meth, usage, ui));
                    j -= 8;
                    if els.len() > 32 { break; }
                }
                els.reverse();
                // require the first element to start at offset 0 in its stream
                while !els.is_empty() && !(els[0].1 == 0) { els.remove(0); }
                if !els.is_empty() {
                    let start = i - els.len() * 8;
                    let va = p.off2va(start).unwrap();
                    let mut stride = std::collections::BTreeMap::new();
                    for e in &els { let end = e.1 as u32 + SIZES[e.2 as usize]; let s = stride.entry(e.0).or_insert(0); if end > *s { *s = end; } }
                    let _ = writeln!(w, "\n## decl @0x{:08x} ({} elements, stride per stream {:?})", va, els.len(), stride);
                    for e in &els { let _ = writeln!(w, "  stream {} off {:3} {:<10} usage {}{}  (method {})", e.0, e.1, TYPES[e.2 as usize], USAGES[e.4 as usize], e.5, e.3); }
                    n += 1;
                }
                i += 8; continue;
            }
            i += 2;
        }
    }
    std::fs::write(out, &w).unwrap();
    println!("declarations: {}", n);
}

//! Research helper for soldier variant files. Read-only. Usage:
//!   cargo run -p ntw_formats --example variant_probe -- vtx <vmpf path> [lod] [count]
//!   cargo run -p ntw_formats --example variant_probe -- bytestats <40|64>
//!   cargo run -p ntw_formats --example variant_probe -- variant <path.unit_variant>
//!   cargo run -p ntw_formats --example variant_probe -- bbox <equipment piece name filter>
use ntw_formats::pack::Vfs;
use ntw_formats::unit_variant::{UnitVariant, VariantPartMesh, VariantPartMeshBody, VariantVertexFormat};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn half(b: &[u8]) -> f32 {
    ntw_formats::unit_variant::f16_to_f32(u16::from_le_bytes([b[0], b[1]]))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    match args[0].as_str() {
        "vtx" => {
            let mesh = VariantPartMesh::read(&vfs.read(&args[1]).unwrap()).unwrap();
            let lod: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(0);
            let n: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(8);
            let VariantPartMeshBody::Part { lods, attachments, .. } = &mesh.body else {
                panic!("container")
            };
            println!("{:?}", mesh.header);
            for (i, l) in lods.iter().enumerate() {
                println!("lod {i}: {} v {} i", l.vertex_count, l.index_count);
            }
            for a in attachments {
                println!("attach {} bone {} {:?}", a.name, a.bone, a.matrix);
            }
            let l = &lods[lod];
            let stride = l.vertices.len() / l.vertex_count as usize;
            for v in l.vertices.chunks(stride).take(n) {
                let hex: Vec<String> = v.chunks(4).map(|c| c.iter().map(|x| format!("{x:02x}")).collect()).collect();
                let halfs: Vec<String> = v.chunks(2).map(|c| format!("{:.3}", half(c))).collect();
                println!("{}\n    {}", hex.join(" "), halfs.join(" "));
            }
        }
        "bytestats" => {
            let stride: usize = args[1].parse().unwrap();
            // per byte offset: min, max, number of distinct values
            let mut seen = vec![[false; 256]; stride];
            let mut count = 0usize;
            for p in vfs.list("variantmodels/unitparts/") {
                if !p.ends_with(".variant_part_mesh") {
                    continue;
                }
                let Ok(mesh) = VariantPartMesh::read(&vfs.read(p).unwrap()) else { continue };
                let VariantPartMeshBody::Part { lods, .. } = &mesh.body else { continue };
                for l in lods {
                    if l.vertices.len() != l.vertex_count as usize * stride {
                        continue;
                    }
                    for v in l.vertices.chunks(stride) {
                        count += 1;
                        for (i, &b) in v.iter().enumerate() {
                            seen[i][b as usize] = true;
                        }
                    }
                }
            }
            println!("{count} vertices");
            for (i, s) in seen.iter().enumerate() {
                let vals: Vec<usize> = (0..256).filter(|&b| s[b]).collect();
                let show: Vec<String> = vals.iter().take(12).map(|b| format!("{b:02x}")).collect();
                println!("byte {i:2}: {:3} distinct  {}", vals.len(), show.join(" "));
            }
        }
        "bbox" => {
            // args: <name filter> ; bone and bounding box of matching equipment pieces
            let filter = args[1].to_ascii_lowercase();
            for p in ["variantmodels/equipment/mesh.variant_part_mesh", "variantmodels/equipment/mesh2.variant_part_mesh"] {
                let mesh = VariantPartMesh::read(&vfs.read(p).unwrap()).unwrap();
                let VariantPartMeshBody::EquipmentContainer { pieces, .. } = &mesh.body else { continue };
                for x in pieces.iter().filter(|x| x.name.to_ascii_lowercase().contains(&filter)) {
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for v in x.lod.decode_vertices(VariantVertexFormat::Bytes64) {
                        for k in 0..3 {
                            lo[k] = lo[k].min(v.positions[0][k]);
                            hi[k] = hi[k].max(v.positions[0][k]);
                        }
                    }
                    println!("{} bone {:?}: {:?} .. {:?}", x.name, x.bone, lo, hi);
                }
            }
        }
        "pieces" => {
            for p in ["variantmodels/equipment/mesh.variant_part_mesh", "variantmodels/equipment/mesh2.variant_part_mesh"] {
                let mesh = VariantPartMesh::read(&vfs.read(p).unwrap()).unwrap();
                let VariantPartMeshBody::EquipmentContainer { pieces, .. } = &mesh.body else { continue };
                let names: Vec<String> = pieces.iter().map(|x| format!("{}@{:?}:{}", x.name, x.bone, x.lod.vertex_count)).collect();
                println!("{p}: {}", names.join(" "));
            }
        }
        "bindcheck" => {
            // args: <anim> [frame] ; mean |M_A pA - M_B pB| over two-bone vertices of some meshes
            use ntw_formats::anim::{Anim, transform_point};
            let a = Anim::read(&vfs.read(&args[1]).unwrap()).unwrap();
            let fr: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(0);
            let w = a.world_matrices(fr);
            let (mut sum, mut n, mut max) = (0f64, 0usize, 0f32);
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in vfs.list("variantmodels/unitparts/euro/") {
                if !p.ends_with(".variant_part_mesh") { continue }
                let Ok(mesh) = VariantPartMesh::read(&vfs.read(p).unwrap()) else { continue };
                let fmt = mesh.header.vertex_format;
                let VariantPartMeshBody::Part { lods, .. } = &mesh.body else { continue };
                if fmt != ntw_formats::unit_variant::VariantVertexFormat::Bytes40 { continue }
                for v in lods.last().unwrap().decode_vertices(fmt) {
                    let [ba, bb] = v.bones.unwrap();
                    if (ba as usize) >= w.len() || (bb as usize) >= w.len() { continue }
                    let pa = transform_point(&w[ba as usize], v.positions[0]);
                    for k in 0..3 { lo[k] = lo[k].min(pa[k]); hi[k] = hi[k].max(pa[k]); }
                    if v.weight > 0.99 { continue }
                    let pb = transform_point(&w[bb as usize], v.positions[1]);
                    let d = ((pa[0]-pb[0]).powi(2) + (pa[1]-pb[1]).powi(2) + (pa[2]-pb[2]).powi(2)).sqrt();
                    sum += d as f64; n += 1; max = max.max(d);
                }
            }
            println!("{n} two-bone vertices: mean gap {:.4} max {:.4}; bbox {lo:?} {hi:?}", sum / n.max(1) as f64, max);
        }
        "maskstats" => {
            let b = vfs.read(&args[1]).unwrap();
            let d = ntw_formats::dds::Dds::parse(&b).unwrap();
            let px = d.decode_rgba8(0);
            let mut hist = [[0usize; 4]; 4];
            for p in px.chunks(4) {
                for c in 0..4 { hist[c][(p[c] / 64) as usize] += 1; }
            }
            println!("{:?} {:?}", d.format, d.level_dims(0));
            for (c, h) in ["R", "G", "B", "A"].iter().zip(hist) { println!("{c} quartiles {h:?}"); }
        }
        "bonepos" => {
            let a = ntw_formats::anim::Anim::read(&vfs.read(&args[1]).unwrap()).unwrap();
            let fr: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(0);
            let w = a.world_matrices(fr);
            for (i, b) in a.bones.iter().enumerate() {
                let p = ntw_formats::anim::transform_point(&w[i], [0.0; 3]);
                println!("{i:2} {:<22} {:7.3} {:7.3} {:7.3}", b.name, p[0], p[1], p[2]);
            }
        }
        "variant" => {
            let v = UnitVariant::read(&vfs.read(&args[1]).unwrap()).unwrap();
            for (i, c) in v.categories.iter().enumerate() {
                println!("[{i}] {:?} index {} meshes {}", c.name, c.index, c.mesh_count);
                for m in v.category_meshes(i).unwrap() {
                    println!("     {:?} kind {} tex {:?}", m.mesh, m.kind, m.texture_stem);
                }
            }
        }
        "normcheck" => {
            // For triangles whose 3 vertices use one bone with weight 1, compare the
            // geometric face normal with the stored normal under two byte orders.
            use ntw_formats::unit_variant::VariantVertexFormat;
            let (mut xyz, mut zyx, mut tri) = (0i64, 0i64, 0usize);
            for p in vfs.list("variantmodels/unitparts/") {
                let Ok(mesh) = VariantPartMesh::read(&vfs.read(p).unwrap()) else { continue };
                let fmt = mesh.header.vertex_format;
                let VariantPartMeshBody::Part { lods, .. } = &mesh.body else { continue };
                for l in lods {
                    let vs = l.decode_vertices(fmt);
                    let raw: Vec<&[u8]> = l.vertices.chunks(fmt.vertex_stride().unwrap()).collect();
                    for t in l.indices.chunks_exact(3) {
                        let [a, b, c] = [t[0], t[1], t[2]].map(|i| &vs[i as usize]);
                        if fmt == VariantVertexFormat::Bytes40 && [a, b, c].iter().any(|v| v.weight < 0.99 || v.bones.unwrap()[0] != a.bones.unwrap()[0]) {
                            continue;
                        }
                        let (p0, p1, p2) = (a.positions[0], b.positions[0], c.positions[0]);
                        let e1 = [p1[0]-p0[0], p1[1]-p0[1], p1[2]-p0[2]];
                        let e2 = [p2[0]-p0[0], p2[1]-p0[1], p2[2]-p0[2]];
                        let n = [e1[1]*e2[2]-e1[2]*e2[1], e1[2]*e2[0]-e1[0]*e2[2], e1[0]*e2[1]-e1[1]*e2[0]];
                        let off = if fmt == VariantVertexFormat::Bytes40 { 24 } else { 16 };
                        let r = raw[t[0] as usize];
                        let u = |x: u8| (x as f32 - 127.5) / 127.5;
                        let d1 = n[0]*u(r[off]) + n[1]*u(r[off+1]) + n[2]*u(r[off+2]);
                        let d2 = n[0]*u(r[off+2]) + n[1]*u(r[off+1]) + n[2]*u(r[off]);
                        xyz += d1.signum() as i64;
                        zyx += d2.signum() as i64;
                        tri += 1;
                    }
                }
            }
            println!("{tri} triangles: sum sign(face.n) xyz {xyz}, zyx {zyx}");
        }
        "anim" => {
            let a = ntw_formats::anim::Anim::read(&vfs.read(&args[1]).unwrap()).unwrap();
            println!("rate {} dur {} bones {} frames {}", a.frame_rate, a.duration, a.bones.len(), a.frames.len());
            if args.get(2).is_some() {
                for (i, b) in a.bones.iter().enumerate() {
                    println!("  {i:2} {:<24} {:?}  {:?}", b.name, b.parent, a.frames[0][i]);
                }
            }
            for e in &a.events {
                println!("  event {e:?}");
            }
        }
        "animfail" => {
            for p in vfs.list("") {
                if p.ends_with(".anim")
                    && let Err(e) = ntw_formats::anim::Anim::read(&vfs.read(p).unwrap())
                {
                    println!("{p}: {e}");
                }
            }
        }
        "animtrail" => {
            let want: usize = args[1].parse().unwrap();
            for p in vfs.list("animations") {
                if let Ok(a) = ntw_formats::anim::Anim::read(&vfs.read(p).unwrap())
                    && a.events.len() == want
                {
                    println!("{p}");
                    break;
                }
            }
        }
        _ => eprintln!("unknown command"),
    }
}

//! Research helper: summarise a **loose** (non-pack) `.rigid_model` from the install — one of the
//! campaign map's own `display\...` files, which the packs do not carry and `pack_probe` cannot
//! reach. Read-only. Prints the meshes, their material texture names, the position and UV bounds,
//! an ASCII top-down occupancy of each mesh and its first vertices, so a flat ground overlay (a
//! campaign arrow, a border strip) can be recognised without a renderer.
//!   cargo run -p ntw_formats --example loose_rigid_probe -- campaign_maps/nap_europe/display/arrows/arrows.rigid_model
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::pack::Vfs;
use ntw_formats::rigid_model::RigidModel;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    let bytes = files.read(&path).expect("read");
    println!("{} bytes", bytes.len());
    let m = RigidModel::read(&bytes).expect("rigid model");
    println!("{} meshes, bbox {:?} .. {:?}", m.meshes.len(), m.bbox_min, m.bbox_max);
    for (i, mesh) in m.meshes.iter().enumerate() {
        let mat = &mesh.material;
        println!(
            "  mesh {i}: v{} verts {} idx {} diffuse {:?} normal {:?} gloss {:?} extra {:?} floats {:?} vec4 {:?}",
            mesh.version, mesh.vertices.len(), mesh.indices.len(), mat.diffuse_name(), mat.normal_name(),
            mat.gloss_name(), mat.extra, mat.float_params, mat.vec4_params
        );
        if let Some(v) = mesh.vertices.first() {
            println!("    v0 {:?} uv {:?} {:?}", v.position, v.uv, v.tangent);
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let (mut ulo, mut uhi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in &mesh.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
            for k in 0..2 {
                ulo[k] = ulo[k].min(v.uv[k]);
                uhi[k] = uhi[k].max(v.uv[k]);
            }
        }
        println!("    pos {lo:?} .. {hi:?}   uv {ulo:?} .. {uhi:?}");
        // Top-down occupancy of the mesh in XZ (looking down -Y), plus the UV corners of the
        // triangles, so the shape and the texture slice are visible without a renderer.
        let (nx, nz) = (72usize, 44usize);
        let (sx, sz) = ((hi[0] - lo[0]), (hi[2] - lo[2]));
        let mut grid = vec![vec![' '; nx]; nz];
        let mut uv_min = [f32::MAX; 2];
        let mut uv_max = [f32::MIN; 2];
        for (n, t) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if n >= 40 {
                println!("    ... {} more triangles", mesh.indices.len() / 3 - n);
                break;
            }
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize]);
            for v in [a, b, c] {
                let gx = (((v.position[0] - lo[0]) / sx) * nx as f32) as isize;
                let gz = (((v.position[2] - lo[2]) / sz) * nz as f32) as isize;
                if (0..nx as isize).contains(&gx) && (0..nz as isize).contains(&gz) {
                    grid[gz as usize][gx as usize] = '#';
                }
                uv_min[0] = uv_min[0].min(v.uv[0]);
                uv_min[1] = uv_min[1].min(v.uv[1]);
                uv_max[0] = uv_max[0].max(v.uv[0]);
                uv_max[1] = uv_max[1].max(v.uv[1]);
            }
        }
        for v in mesh.vertices.iter().take(40) {
            println!(
                "    v x={:9.4} y={:9.5} z={:9.4}  uv=({:.4},{:.4})",
                v.position[0], v.position[1], v.position[2], v.uv[0], v.uv[1]
            );
        }
        for row in &grid {
            println!("    |{}|", row.iter().collect::<String>());
        }
        println!("    UV span {uv_min:?} .. {uv_max:?}");
    }
}

//! Research helper: print a summary of one `.rigid_model` from the install (read-only).
//!   cargo run -p ntw_formats --example rigid_info -- <vfs path>
use ntw_formats::pack::Vfs;
use ntw_formats::rigid_model::RigidModel;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let m = RigidModel::read(&vfs.read(&path).unwrap()).unwrap();
    println!("{} meshes, bbox {:?} .. {:?}", m.meshes.len(), m.bbox_min, m.bbox_max);
    for (i, mesh) in m.meshes.iter().enumerate() {
        let mat = &mesh.material;
        println!(
            "  mesh {i}: v{} verts {} idx {} diffuse {:?} normal {:?} gloss {:?} extra {:?} floats {:?} vec4 {:?}",
            mesh.version, mesh.vertices.len(), mesh.indices.len(), mat.diffuse_name(), mat.normal_name(),
            mat.gloss_name(), mat.extra, mat.float_params, mat.vec4_params
        );
        // Winding check: does (b-a) x (c-a) point along the stored vertex normal?
        let (mut along, mut against) = (0, 0);
        for t in mesh.indices.as_chunks::<3>().0.iter() {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize]);
            let e1 = [0, 1, 2].map(|k| b.position[k] - a.position[k]);
            let e2 = [0, 1, 2].map(|k| c.position[k] - a.position[k]);
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let vn = [0, 1, 2].map(|k| a.normal[k] + b.normal[k] + c.normal[k]);
            if n[0] * vn[0] + n[1] * vn[1] + n[2] * vn[2] >= 0.0 { along += 1 } else { against += 1 }
        }
        println!("    triangles whose (b-a)x(c-a) is along the vertex normal: {along}, against: {against}");
        if let Some(v) = mesh.vertices.first() {
            println!("    first vertex {v:?}");
        }
    }
}

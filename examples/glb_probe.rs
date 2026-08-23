//! EXT dev probe: verify the `gltf` crate API against the door asset before wiring it in.
//! Run: cargo run --release --example glb_probe -- <path.glb>
fn main() {
    let path = std::env::args().nth(1).expect("usage: glb_probe <file.glb>");
    // Same entry point src/ext/gltf_model.rs uses -- gltf::import is deliberately not enabled,
    // because it decodes every image at once.
    let bytes = std::fs::read(&path).expect("read");
    let g = gltf::Gltf::from_slice(&bytes).expect("parse");
    let blob = g.blob.clone().expect("no BIN chunk");
    let doc = g.document;
    let buffers = [blob.clone()];
    for (i, img) in doc.images().enumerate() {
        match img.source() {
            gltf::image::Source::View { view, mime_type } => {
                println!("  image {i} {:?}: {mime_type} bytes={}", img.name(), view.length())
            }
            gltf::image::Source::Uri { uri, .. } => println!("  image {i}: uri {uri}"),
        }
    }
    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let r = prim.reader(|b| buffers.get(b.index()).map(|v| v.as_slice()));
            let np = r.read_positions().map(|v| v.count()).unwrap_or(0);
            let nn = r.read_normals().map(|v| v.count()).unwrap_or(0);
            let nt = r.read_tangents().map(|v| v.count()).unwrap_or(0);
            let nuv = r.read_tex_coords(0).map(|v| v.into_f32().count()).unwrap_or(0);
            let ni = r.read_indices().map(|v| v.into_u32().count()).unwrap_or(0);
            let m = prim.material();
            let pbr = m.pbr_metallic_roughness();
            println!(
                "  prim {:?} mat={:?} pos={np} nrm={nn} tan={nt} uv={nuv} idx={ni} mode={:?}",
                mesh.name(), m.name(), prim.mode()
            );
            println!(
                "    base={:?} bc_tex={:?} metal={} rough={} mr_tex={:?} normal_tex={:?}(scale {:?}) occl_tex={:?}(str {:?}) double={} alpha={:?}",
                pbr.base_color_factor(),
                pbr.base_color_texture().map(|t| t.texture().source().index()),
                pbr.metallic_factor(), pbr.roughness_factor(),
                pbr.metallic_roughness_texture().map(|t| t.texture().source().index()),
                m.normal_texture().map(|t| t.texture().source().index()),
                m.normal_texture().map(|t| t.scale()),
                m.occlusion_texture().map(|t| t.texture().source().index()),
                m.occlusion_texture().map(|t| t.strength()),
                m.double_sided(), m.alpha_mode()
            );
        }
    }
    for node in doc.nodes() {
        let t = node.transform().decomposed();
        println!("  node {} {:?} mesh={:?} T={:?} R={:?} S={:?} children={:?}",
            node.index(), node.name(), node.mesh().map(|m| m.index()),
            t.0, t.1, t.2, node.children().map(|c| c.index()).collect::<Vec<_>>());
    }
}

// Port of Mesh.h / Mesh.cpp

use std::rc::Rc;

// EXT: typed loader failures and the asset root (see src/app/).
use crate::app::assets;
use crate::app::error::AssetError;
use crate::collider::Collider;
use crate::vector::Vector3;

// static const int NUM_VBOS = 3; (Mesh.h:10)
pub const NUM_VBOS: usize = 3;

// PORT: STRUCTURAL LIBERTY (the only one taken in this file, and the one the brief allows):
// Mesh::Mesh parses the .obj straight into the object's own `verts`/`uvs`/`normals` members
// and then uploads them (Mesh.cpp:8-153). Here the parse is factored into the pure function
// `parse_obj`, which touches no OpenGL, so the .obj reader can be unit-tested without a GL
// context. `Mesh::new` calls `parse_obj` and then performs exactly the upload block of
// Mesh.cpp:130-152. `ParsedMesh` holds what were the C++ member variables
// (Mesh.h:29-31) plus `is3DTex`, which in C++ was a constructor local (Mesh.cpp:18) that
// the upload block read directly (Mesh.cpp:145).
#[derive(Default)]
pub struct ParsedMesh {
    pub colliders: Vec<Collider>,
    pub verts: Vec<f32>,
    pub uvs: Vec<f32>,
    pub normals: Vec<f32>,
    pub is_3d_tex: bool,
}

// PORT: stand-in for std::istringstream's `operator>>` chain (Mesh.cpp:25, 32, 47, 73).
// It reproduces the two behaviours the parser actually depends on:
//   * a failed extraction sets the fail flag and zeroes the target (C++11 num_get behaviour),
//     which is what `ss.fail()` is tested for at Mesh.cpp:37 and Mesh.cpp:94;
//   * once the fail flag is set every later extraction is a no-op.
// (C++ leaves the target *unmodified* on an already-failed stream rather than zeroing it;
// every call site here gates on the fail flag before using those values, so the difference
// is unobservable.)
// PORT: C++ `>>` consumes the longest valid numeric *prefix* of a token; this reads the
// whole whitespace-delimited token and parses it. All 14 shipped .obj files use clean
// whitespace-separated numbers, so the two agree.
struct SStream<'a> {
    s: &'a [u8],
    pos: usize,
    fail: bool,
    /// Treat `/` as a separator as well as whitespace.
    ///
    /// PORT: the C++ overwrites every '/' in an `f` line with a space before parsing it
    /// (Mesh.cpp:61) so that `>>` splits `1/2` into two integers. Doing that literally means
    /// copying the line to mutate it, which on a million-face mesh is a million heap allocations
    /// for a rewrite that only ever feeds this tokenizer. Splitting on the character instead
    /// produces the same tokens without touching the input. It is opt-in rather than always on
    /// so that `v`/`vt`/`c` lines keep failing on a stray '/' exactly as the stream they
    /// stand in for would.
    slash_splits: bool,
}

impl<'a> SStream<'a> {
    fn new(s: &'a str) -> SStream<'a> {
        SStream::from_bytes(s.as_bytes(), false)
    }

    fn from_bytes(s: &'a [u8], slash_splits: bool) -> SStream<'a> {
        SStream { s, pos: 0, fail: false, slash_splits }
    }

    fn is_sep(&self, b: u8) -> bool {
        b.is_ascii_whitespace() || (self.slash_splits && b == b'/')
    }

    fn next_token(&mut self) -> Option<&'a [u8]> {
        while self.pos < self.s.len() && self.is_sep(self.s[self.pos]) {
            self.pos += 1;
        }
        if self.pos >= self.s.len() {
            return None;
        }
        let start = self.pos;
        while self.pos < self.s.len() && !self.is_sep(self.s[self.pos]) {
            self.pos += 1;
        }
        Some(&self.s[start..self.pos])
    }

    fn extract_f32(&mut self) -> f32 {
        if self.fail {
            return 0.0;
        }
        match self
            .next_token()
            .and_then(|t| std::str::from_utf8(t).ok())
            .and_then(|t| t.parse::<f32>().ok())
        {
            Some(v) => v,
            None => {
                self.fail = true;
                0.0
            }
        }
    }

    fn extract_u32(&mut self) -> u32 {
        if self.fail {
            return 0;
        }
        match self
            .next_token()
            .and_then(|t| std::str::from_utf8(t).ok())
            .and_then(|t| t.parse::<u32>().ok())
        {
            Some(v) => v,
            None => {
                self.fail = true;
                0
            }
        }
    }

    fn is_fail(&self) -> bool {
        self.fail
    }
}

// PORT: std::string::operator[] returns '\0' for index == size(); this reproduces that so
// `line[2]` / `line[3]` on a short line behave the same (Mesh.cpp:43, 74, 75).
#[inline]
fn char_at(b: &[u8], i: usize) -> u8 {
    if i < b.len() {
        b[i]
    } else {
        0
    }
}

impl ParsedMesh {
    // Mesh::AddFace (Mesh.cpp:171-215)
    // Ten with `self`: the C++ signature's nine -- the two palettes, three vertex/uv index pairs
    // and the 3D-texture flag -- kept as they are because the body mutates the six indices by
    // name; a struct would only move them.
    #[allow(clippy::too_many_arguments)]
    fn add_face(
        &mut self,
        vert_palette: &[f32],
        uv_palette: &[f32],
        mut a: u32,
        mut at: u32,
        mut b: u32,
        mut bt: u32,
        mut c: u32,
        mut ct: u32,
        is_3d_tex: bool,
    ) {
        //Merge texture and vertex indicies
        debug_assert!(a > 0 && b > 0 && c > 0);
        debug_assert!(at > 0 && bt > 0 && ct > 0);
        // PORT: wrapping_sub keeps C++'s unsigned wraparound for the (asserted-away in
        // release) index-0 case (was: a -= 1; ..., Mesh.cpp:178-179).
        a = a.wrapping_sub(1);
        b = b.wrapping_sub(1);
        c = c.wrapping_sub(1);
        at = at.wrapping_sub(1);
        bt = bt.wrapping_sub(1);
        ct = ct.wrapping_sub(1);
        let v_ix: [u32; 3] = [a, b, c];
        let uv_ix: [u32; 3] = [at, bt, ct];

        //Calcuate the normal for this face
        let v1 = Vector3::from_slice(&vert_palette[(a as usize) * 3..]);
        let v2 = Vector3::from_slice(&vert_palette[(b as usize) * 3..]);
        let v3 = Vector3::from_slice(&vert_palette[(c as usize) * 3..]);
        let normal = (v2 - v1).cross(v3 - v1).normalized();

        for i in 0..3 {
            let v = v_ix[i] as usize;
            let vt = uv_ix[i] as usize;
            debug_assert!(v < vert_palette.len() / 3);
            self.verts.push(vert_palette[v * 3]);
            self.verts.push(vert_palette[v * 3 + 1]);
            self.verts.push(vert_palette[v * 3 + 2]);
            if !uv_palette.is_empty() {
                if is_3d_tex {
                    debug_assert!(vt < uv_palette.len() / 3);
                    self.uvs.push(uv_palette[vt * 3]);
                    self.uvs.push(uv_palette[vt * 3 + 1]);
                    self.uvs.push(uv_palette[vt * 3 + 2]);
                } else {
                    debug_assert!(vt < uv_palette.len() / 2);
                    self.uvs.push(uv_palette[vt * 2]);
                    self.uvs.push(uv_palette[vt * 2 + 1]);
                }
            } else {
                self.uvs.push(0.0);
                self.uvs.push(0.0);
            }
            self.normals.push(normal.x);
            self.normals.push(normal.y);
            self.normals.push(normal.z);
        }
    }
}

// Body of Mesh::Mesh's read loop (Mesh.cpp:15-128), verbatim apart from the noted deviations.
// The `unused_assignments` allow keeps the C++'s `uint32_t a=0, b=0, c=0, d=0;` zero-initializers
// (Mesh.cpp:70-71) rather than dropping them; every branch below overwrites them.
#[allow(unused_assignments)]
pub fn parse_obj(text: &str) -> ParsedMesh {
    let mut out = ParsedMesh::default();

    //Temporaries
    let mut vert_palette: Vec<f32> = Vec::new();
    let mut uv_palette: Vec<f32> = Vec::new();
    let mut is_3d_tex = false;

    //Read the file
    // PORT: `while (!fin.eof()) { std::getline(fin, line); ... }` (Mesh.cpp:22-23) becomes a
    // `.lines()` iteration. The C++ loop performs one extra iteration on an empty `line` when
    // the file ends in a newline, which matches no prefix and is a no-op. `str::lines` also
    // strips a trailing '\r', matching Windows text-mode getline.
    for line in text.lines() {
        let lb = line.as_bytes();
        if let Some(rest) = line.strip_prefix("v ") {
            let mut ss = SStream::new(rest);
            let x = ss.extract_f32();
            let y = ss.extract_f32();
            let z = ss.extract_f32();
            vert_palette.push(x);
            vert_palette.push(y);
            vert_palette.push(z);
        } else if let Some(rest) = line.strip_prefix("vt ") {
            let mut ss = SStream::new(rest);
            let u = ss.extract_f32();
            let v = ss.extract_f32();
            let w = ss.extract_f32();
            uv_palette.push(u);
            uv_palette.push(v);
            if !ss.is_fail() {
                uv_palette.push(w);
                is_3d_tex = true;
            }
        } else if let Some(rest) = line.strip_prefix("c ") {
            let a: u32;
            let b: u32;
            let c: u32;
            if char_at(lb, 2) == b'*' {
                let v_ix = (vert_palette.len() / 3) as u32;
                a = v_ix - 2;
                b = v_ix - 1;
                c = v_ix;
            } else {
                let mut ss = SStream::new(rest);
                a = ss.extract_u32();
                b = ss.extract_u32();
                c = ss.extract_u32();
            }
            // PORT: wrapping_sub for the C++ unsigned `(a - 1) * 3` pointer offset
            // (Mesh.cpp:50-52).
            let v1 = Vector3::from_slice(&vert_palette[(a.wrapping_sub(1) as usize) * 3..]);
            let v2 = Vector3::from_slice(&vert_palette[(b.wrapping_sub(1) as usize) * 3..]);
            let v3 = Vector3::from_slice(&vert_palette[(c.wrapping_sub(1) as usize) * 3..]);
            out.colliders.push(Collider::new(v1, v2, v3));
        } else if line.starts_with("f ") {
            //Count the slashes
            // PORT: the C++ mutates `line` in place, replacing '/' with ' ' (Mesh.cpp:61). Here
            // the line is only read: the slashes are counted where they are, and the tokenizer
            // below is told to split on them (`SStream::slash_splits`). The rewrite existed
            // solely to make `>>` split, and copying a line per face to perform it cost two
            // heap allocations each -- two million of them on the old 124 MB grass patch alone.
            let mut num_slashes = 0i32;
            let mut last_slash_ix: usize = 0;
            let mut doubleslash = false;
            for (i, &ch) in lb.iter().enumerate() {
                if ch == b'/' {
                    // PORT: wrapping_sub reproduces size_t's wraparound for i == 0 (was:
                    // last_slash_ix == i - 1, Mesh.cpp:62). i is never 0 for an "f " line.
                    if last_slash_ix == i.wrapping_sub(1) {
                        debug_assert!(
                            vert_palette.len() == uv_palette.len() || uv_palette.is_empty()
                        );
                        doubleslash = true;
                    }
                    last_slash_ix = i;
                    num_slashes += 1;
                }
            }
            let mut a: u32 = 0;
            let mut b: u32 = 0;
            let mut c: u32 = 0;
            let mut d: u32 = 0;
            let mut at: u32 = 0;
            let mut bt: u32 = 0;
            let mut ct: u32 = 0;
            let mut dt: u32 = 0;
            let mut _tmp: u32;
            let mut ss = SStream::from_bytes(&lb[2..], true);
            let wild = char_at(lb, 2) == b'*';
            let wild2 = char_at(lb, 3) == b'*';
            let mut is_quad = false;

            //Interpret face based on slash
            if wild {
                debug_assert!(num_slashes == 0);
                let v_ix = (vert_palette.len() / 3) as u32;
                let t_ix = (uv_palette.len() / if is_3d_tex { 3 } else { 2 }) as u32;
                if wild2 {
                    // PORT: wrapping_sub keeps the C++ unsigned underflow behaviour when the
                    // palettes hold fewer than 4 entries (Mesh.cpp:84-85).
                    a = v_ix.wrapping_sub(3);
                    b = v_ix.wrapping_sub(2);
                    c = v_ix.wrapping_sub(1);
                    d = v_ix.wrapping_sub(0);
                    at = t_ix.wrapping_sub(3);
                    bt = t_ix.wrapping_sub(2);
                    ct = t_ix.wrapping_sub(1);
                    dt = t_ix.wrapping_sub(0);
                    is_quad = true;
                } else {
                    a = v_ix.wrapping_sub(2);
                    b = v_ix.wrapping_sub(1);
                    c = v_ix;
                    at = t_ix.wrapping_sub(2);
                    bt = t_ix.wrapping_sub(1);
                    ct = t_ix;
                }
            } else if num_slashes == 0 {
                a = ss.extract_u32();
                b = ss.extract_u32();
                c = ss.extract_u32();
                d = ss.extract_u32();
                at = a;
                bt = b;
                ct = c;
                dt = d;
                if !ss.is_fail() {
                    is_quad = true;
                }
            } else if num_slashes == 3 {
                a = ss.extract_u32();
                at = ss.extract_u32();
                b = ss.extract_u32();
                bt = ss.extract_u32();
                c = ss.extract_u32();
                ct = ss.extract_u32();
            } else if num_slashes == 4 {
                is_quad = true;
                a = ss.extract_u32();
                at = ss.extract_u32();
                b = ss.extract_u32();
                bt = ss.extract_u32();
                c = ss.extract_u32();
                ct = ss.extract_u32();
                d = ss.extract_u32();
                dt = ss.extract_u32();
            } else if num_slashes == 6 {
                if doubleslash {
                    a = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    b = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    c = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    let _ = _tmp;
                    at = a;
                    bt = b;
                    ct = c;
                } else {
                    a = ss.extract_u32();
                    at = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    b = ss.extract_u32();
                    bt = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    c = ss.extract_u32();
                    ct = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    let _ = _tmp;
                }
            } else if num_slashes == 8 {
                is_quad = true;
                if doubleslash {
                    a = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    b = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    c = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    d = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    let _ = _tmp;
                    at = a;
                    bt = b;
                    ct = c;
                    dt = d;
                } else {
                    a = ss.extract_u32();
                    at = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    b = ss.extract_u32();
                    bt = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    c = ss.extract_u32();
                    ct = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    d = ss.extract_u32();
                    dt = ss.extract_u32();
                    _tmp = ss.extract_u32();
                    let _ = _tmp;
                }
            } else {
                debug_assert!(false);
                continue;
            }

            //Add face to list
            out.add_face(&vert_palette, &uv_palette, a, at, b, bt, c, ct, is_3d_tex);
            if is_quad {
                out.add_face(&vert_palette, &uv_palette, c, ct, d, dt, a, at, is_3d_tex);
            }
        }
    }

    out.is_3d_tex = is_3d_tex;
    out
}

pub struct Mesh {
    pub colliders: Vec<Collider>,
    vao: glow::VertexArray,
    vbo: [glow::Buffer; NUM_VBOS],
    // PORT: the C++ keeps `verts`/`uvs`/`normals` alive for the lifetime of the Mesh purely
    // so Draw() can read verts.size() (Mesh.h:29-31, Mesh.cpp:162). Only the count survives
    // the upload here.
    vert_count: i32,
    gl: Rc<glow::Context>,

    // EXT: a CPU copy of the triangles (local space) and the mesh's bounding radius. The port
    // drops vertex data after upload (see above); the held-object fit in ext/grab.rs needs
    // real geometry to test against, because most meshes carry no colliders at all (bunny,
    // teapot, suzanne, and every ceiling in the original rooms) and a collider-only test lets
    // held objects punch through them.
    //
    // EMPTY for meshes past `ext::grab::FIT_TRI_CAP`, which that test skips anyway. Scenery at
    // that size is never a grab target and never a thing a held object is fitted against, so a
    // list for it is memory and load time spent on something unreadable.
    pub tris: Vec<[crate::vector::Vector3; 3]>,
    /// EXT: max |v| over all vertices -- a conservative bounding-sphere radius at unit scale.
    pub bound_radius: f32,
}

impl Mesh {
    // Mesh::Mesh (Mesh.cpp:8-153)
    // EXT: returns the failure instead of drawing nothing; `Resources::acquire_mesh` is where
    // it becomes fatal.
    pub fn new(gl: &Rc<glow::Context>, fname: &str) -> Result<Mesh, AssetError> {
        use glow::HasContext;

        //Open the file for reading
        // PORT: on a failed open the C++ `return`s from the constructor before any of the
        // glGen* calls, leaving `vao`/`vbo` uninitialized and Draw() reading garbage handles
        // (Mesh.cpp:10-13). Here the failure is an error: a mesh that is asked for and not
        // shipped is a packaging bug, and an invisible object is the worst way to learn of it.
        // EXT: under the resolved asset root rather than the working directory.
        let path = assets::path(&format!("Meshes/{}", fname));
        let parsed = std::fs::read_to_string(&path)
            .map(|text| parse_obj(&text))
            .map_err(|source| AssetError::Io { path, source })?;

        let verts = parsed.verts;
        // EXT: build the CPU triangle list and bounding radius before the upload consumes
        // `verts`. The expanded vertex list is 9 floats per triangle (Mesh.cpp:189-214).
        let (tris, bound_radius) = {
            use crate::vector::Vector3;
            // The radius is a max over the vertices: no memory, and every grab test reads it.
            let mut r2: f32 = 0.0;
            for v in verts.chunks_exact(3) {
                r2 = r2.max(Vector3::new(v[0], v[1], v[2]).mag_sq());
            }
            // The triangle list only for meshes small enough that `grab::fits` will look at
            // it. Past FIT_TRI_CAP it falls back to colliders and can never read this, and the
            // meshes past it are the enormous ones -- the grass patch's two million triangles
            // would be 75 MB of list, built a triangle at a time, that nothing may consult.
            let n_tris = verts.len() / 9;
            let mut tris = Vec::new();
            if n_tris <= crate::ext::grab::FIT_TRI_CAP {
                tris.reserve_exact(n_tris);
                for t in verts.chunks_exact(9) {
                    tris.push([
                        Vector3::new(t[0], t[1], t[2]),
                        Vector3::new(t[3], t[4], t[5]),
                        Vector3::new(t[6], t[7], t[8]),
                    ]);
                }
            }
            (tris, r2.sqrt())
        };
        let uvs = parsed.uvs;
        let normals = parsed.normals;
        let is_3d_tex = parsed.is_3d_tex;

        //Setup GL
        let gl_error =
            |e: String| AssetError::Gl(format!("buffer allocation for '{}' failed: {}", fname, e));
        unsafe {
            let vao = gl.create_vertex_array().map_err(gl_error)?;
            gl.bind_vertex_array(Some(vao));

            let vbo: [glow::Buffer; NUM_VBOS] = [
                gl.create_buffer().map_err(gl_error)?,
                gl.create_buffer().map_err(gl_error)?,
                gl.create_buffer().map_err(gl_error)?,
            ];
            {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo[0]));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&verts), glow::STATIC_DRAW);
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
            }
            {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo[1]));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&uvs), glow::STATIC_DRAW);
                gl.enable_vertex_attrib_array(1);
                gl.vertex_attrib_pointer_f32(
                    1,
                    if is_3d_tex { 3 } else { 2 },
                    glow::FLOAT,
                    false,
                    0,
                    0,
                );
            }
            {
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo[2]));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&normals), glow::STATIC_DRAW);
                gl.enable_vertex_attrib_array(2);
                gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, 0, 0);
            }

            Ok(Mesh {
                tris,
                bound_radius,
                colliders: parsed.colliders,
                vao,
                vbo,
                // PORT: BUG FIX (approved). The C++ passes `(GLsizei)verts.size()` -- the
                // number of *floats* -- as the vertex count, i.e. 3x the real vertex count,
                // so glDrawArrays reads two vertex-strides past the end of every buffer.
                // (was: glDrawArrays(GL_TRIANGLES, 0, (GLsizei)verts.size()), Mesh.cpp:162)
                vert_count: (verts.len() / 3) as i32,
                gl: Rc::clone(gl),
            })
        }
    }

    /// EXT: a mesh that is nothing but colliders -- the in-memory twin of
    /// `intro_door_collide.obj`, for scene code that knows its rectangles as numbers and has
    /// no file to parse. No faces: the VAO is empty and `draw` issues a draw of zero
    /// vertices, `bound_radius` is 0 so the cull (`ext::cull::object_sphere`) leaves it
    /// alone, and the collision pass reads `colliders` exactly as it would off a loaded mesh.
    /// The three buffers exist, empty, so `Drop` has nothing to special-case.
    #[allow(dead_code)] // EXT: scene code builds its collision-only props with it.
    pub fn colliders_only(gl: &Rc<glow::Context>, colliders: Vec<Collider>) -> Mesh {
        use glow::HasContext;
        fn fatal<T>(e: String) -> T {
            crate::app::crash::fatal(&AssetError::Gl(format!(
                "buffer allocation for a collider-only mesh failed: {e}"
            )))
        }
        unsafe {
            let vao = gl.create_vertex_array().unwrap_or_else(fatal);
            let vbo = [
                gl.create_buffer().unwrap_or_else(fatal),
                gl.create_buffer().unwrap_or_else(fatal),
                gl.create_buffer().unwrap_or_else(fatal),
            ];
            Mesh {
                colliders,
                vao,
                vbo,
                vert_count: 0,
                gl: Rc::clone(gl),
                tris: Vec::new(),
                bound_radius: 0.0,
            }
        }
    }

    // Mesh::Draw (Mesh.cpp:160-163)
    pub fn draw(&self) {
        use glow::HasContext;
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, self.vert_count);
        }
    }

    // PORT: Mesh::DebugDraw (Mesh.cpp:165-169) is dropped along with Collider::DebugDraw,
    // which it is a thin loop over -- that one is immediate mode and cannot exist in a Core
    // profile. Nothing in the codebase calls Mesh::DebugDraw.
    // (was: void DebugDraw(const Camera& cam, const Matrix4& objMat), Mesh.h:17)
}

// Mesh::~Mesh (Mesh.cpp:155-158)
impl Drop for Mesh {
    fn drop(&mut self) {
        use glow::HasContext;
        unsafe {
            for b in self.vbo {
                self.gl.delete_buffer(b);
            }
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

// PORT: glBufferData takes a void* + byte count; glow's buffer_data_u8_slice takes &[u8].
#[inline]
fn as_bytes(v: &[f32]) -> &[u8] {
    bytemuck::cast_slice(v)
}

#[cfg(test)]
mod tests {
    use super::parse_obj;

    fn load(name: &str) -> super::ParsedMesh {
        let path = format!("{}/Meshes/{}", env!("CARGO_MANIFEST_DIR"), name);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path, e));
        parse_obj(&text)
    }

    #[test]
    fn collider_counts() {
        assert_eq!(load("tunnel.obj").colliders.len(), 10);
        assert_eq!(load("square_rooms.obj").colliders.len(), 33);
        assert_eq!(load("floorplan.obj").colliders.len(), 85);
        assert_eq!(load("ground.obj").colliders.len(), 1);
        assert_eq!(load("pillar.obj").colliders.len(), 4);
        assert_eq!(load("ground_slope.obj").colliders.len(), 3);
    }

    #[test]
    fn all_meshes_parse() {
        // Every shipped mesh must parse without tripping a debug assertion, and must produce
        // a vertex count that is a whole number of triangles with matching uv/normal streams.
        for name in [
            "bunny.obj",
            "double_quad.obj",
            "floorplan.obj",
            "ground.obj",
            "ground_slope.obj",
            "pillar.obj",
            "pillar_room.obj",
            "quad.obj",
            "square_rooms.obj",
            "suzanne.obj",
            "teapot.obj",
            "tunnel.obj",
            "tunnel_scale.obj",
            "tunnel_slope.obj",
        ] {
            let m = load(name);
            let n = m.verts.len() / 3;
            assert!(n > 0, "{} produced no vertices", name);
            assert_eq!(m.verts.len() % 9, 0, "{} is not whole triangles", name);
            assert_eq!(m.normals.len(), m.verts.len(), "{} normal count", name);
            let uv_stride = if m.is_3d_tex { 3 } else { 2 };
            assert_eq!(m.uvs.len(), n * uv_stride, "{} uv count", name);
        }
    }

    #[test]
    fn is_3d_tex_detection() {
        // 3-component "vt u v w" lines -> is_3d_tex.
        assert!(load("pillar.obj").is_3d_tex);
        assert!(load("floorplan.obj").is_3d_tex);
        assert!(load("pillar_room.obj").is_3d_tex);
        // 2-component "vt u v" lines -> stays false.
        assert!(!load("tunnel.obj").is_3d_tex);
        assert!(!load("ground.obj").is_3d_tex);
        assert!(!load("square_rooms.obj").is_3d_tex);
        // No "vt" lines at all -> stays false, and uvs fall back to (0,0) pairs.
        assert!(!load("bunny.obj").is_3d_tex);
        assert!(!load("teapot.obj").is_3d_tex);
    }
}

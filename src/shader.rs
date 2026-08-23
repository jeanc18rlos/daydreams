//! Port of Shader.h / Shader.cpp.

use std::rc::Rc;

use glow::HasContext;

use crate::vector::Matrix4;

pub struct Shader {
    // PORT: `std::vector<std::string> attribs` (Shader.h:17) is not a field. It is only ever
    // written by LoadShader and read by the constructor, so it becomes a local threaded into
    // load_shader (was: std::vector<std::string> attribs, Shader.h:17).
    prog: glow::Program,
    vert: glow::Shader,
    frag: glow::Shader,
    mvp_id: Option<glow::UniformLocation>,
    mv_id: Option<glow::UniformLocation>,
    gl: Rc<glow::Context>,
}

// PORT: `str.find(pat, from)` has no direct std equivalent (was: str.find("\nin ", ix), Shader.cpp:95).
fn find_from(s: &str, pat: &str, from: usize) -> Option<usize> {
    s[from..].find(pat).map(|i| i + from)
}

impl Shader {
    pub fn new(gl: &Rc<glow::Context>, name: &str) -> Shader {
        //Get the file paths
        let vert = format!("Shaders/{}.vert", name);
        let frag = format!("Shaders/{}.frag", name);

        // PORT: the attribs member (Shader.h:17) is a local here, see the note on the struct.
        let mut attribs: Vec<String> = Vec::new();

        //Load the shaders from disk
        let vert_id = Self::load_shader(gl, &vert, glow::VERTEX_SHADER, &mut attribs);
        let frag_id = Self::load_shader(gl, &frag, glow::FRAGMENT_SHADER, &mut attribs);

        unsafe {
            //Create the program
            let prog_id = gl
                .create_program()
                .unwrap_or_else(|e| panic!("glCreateProgram failed for '{}': {}", name, e));
            gl.attach_shader(prog_id, vert_id);
            gl.attach_shader(prog_id, frag_id);

            //Bind variables
            for (i, a) in attribs.iter().enumerate() {
                gl.bind_attrib_location(prog_id, i as u32, a);
            }

            //Link the program
            gl.link_program(prog_id);

            //Check for linking errors
            let is_linked = gl.get_program_link_status(prog_id);
            if !is_linked {
                // PORT: the C++ writes the info log to "<vert>.link.log", sets progId = 0 and
                // returns, silently rendering nothing. We print the log to stderr and panic,
                // which is far easier to debug (was: std::ofstream fout(std::string(vert) +
                // ".link.log"); ... progId = 0; return;, Shader.cpp:31-43).
                let log = gl.get_program_info_log(prog_id);
                eprintln!("{}.link.log:\n{}", vert, log);
                panic!("Failed to link shader program '{}'", name);
            }

            //Get global variable locations
            let mvp_id = gl.get_uniform_location(prog_id, "mvp");
            let mv_id = gl.get_uniform_location(prog_id, "mv");

            Shader {
                prog: prog_id,
                vert: vert_id,
                frag: frag_id,
                mvp_id,
                mv_id,
                gl: Rc::clone(gl),
            }
        }
    }

    // PORT: renamed from Use (a reserved-ish name collision with the `use` keyword is avoided)
    // (was: void Shader::Use(), Shader.cpp:58).
    pub fn use_program(&self) {
        unsafe {
            self.gl.use_program(Some(self.prog));
        }
    }

    // PORT: `GLuint LoadShader(const char*, GLenum)` was a member fn so it could push onto the
    // `attribs` member; here the vector is passed in explicitly (was: GLuint Shader::LoadShader,
    // Shader.cpp:62).
    fn load_shader(
        gl: &Rc<glow::Context>,
        fname: &str,
        ty: u32,
        attribs: &mut Vec<String>,
    ) -> glow::Shader {
        //Read shader source from disk
        // PORT: a failed std::ifstream silently yields an empty source string (which then fails
        // to compile and writes a .log); we panic with the io error instead
        // (was: std::ifstream fin(fname); std::stringstream buff; buff << fin.rdbuf();,
        // Shader.cpp:64-68).
        let str = std::fs::read_to_string(fname)
            .unwrap_or_else(|e| panic!("Failed to open shader '{}': {}", fname, e));

        unsafe {
            //Create and compile shader
            let id = gl
                .create_shader(ty)
                .unwrap_or_else(|e| panic!("glCreateShader failed for '{}': {}", fname, e));
            gl.shader_source(id, &str);
            gl.compile_shader(id);

            //Check to make sure there were no errors
            let is_compiled = gl.get_shader_compile_status(id);
            if !is_compiled {
                // PORT: the C++ writes the info log to "<fname>.log" and returns 0, leaving the
                // program to link against a null shader. We print the log to stderr and panic
                // (was: std::ofstream fout(std::string(fname) + ".log"); ... return 0;,
                // Shader.cpp:79-89).
                let log = gl.get_shader_info_log(id);
                eprintln!("{}.log:\n{}", fname, log);
                panic!("Failed to compile shader '{}'", fname);
            }

            //Save variable bindings
            if ty == glow::VERTEX_SHADER {
                let bytes = str.as_bytes();
                let mut ix: usize = 0;
                loop {
                    ix = match find_from(&str, "\nin ", ix) {
                        Some(i) => i,
                        None => break,
                    };
                    // PORT: if there is no ';' after "\nin " the C++ walks backwards from npos
                    // (undefined behaviour); we panic (was: ix = str.find(";", ix);,
                    // Shader.cpp:99).
                    ix = find_from(&str, ";", ix)
                        .unwrap_or_else(|| panic!("Malformed 'in' declaration in '{}'", fname));
                    let mut start_ix = ix;
                    //while (str[--start_ix] != ' ');
                    loop {
                        start_ix -= 1;
                        if bytes[start_ix] == b' ' {
                            break;
                        }
                    }
                    attribs.push(str[start_ix + 1..ix].to_string());
                }
            }

            //Return the shader id
            id
        }
    }

    // PORT: `void SetMVP(const float* mvp, const float* mv)` took raw pointers that could be
    // null; here they are Option<&Matrix4> (was: void Shader::SetMVP(const float*, const float*),
    // Shader.cpp:110).
    pub fn set_mvp(&self, mvp: Option<&Matrix4>, mv: Option<&Matrix4>) {
        unsafe {
            //GL_TRUE: Matrix4 is row-major, so the upload must transpose.
            if let Some(mvp) = mvp {
                self.gl
                    .uniform_matrix_4_f32_slice(self.mvp_id.as_ref(), true, &mvp.m);
            }
            if let Some(mv) = mv {
                self.gl
                    .uniform_matrix_4_f32_slice(self.mv_id.as_ref(), true, &mv.m);
            }
        }
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe {
            self.gl.detach_shader(self.prog, self.vert);
            self.gl.detach_shader(self.prog, self.frag);
            self.gl.delete_program(self.prog);
            self.gl.delete_shader(self.vert);
            self.gl.delete_shader(self.frag);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXT: additions beyond the C++ port. The original exposes exactly two uniforms,
// mvp and mv (Shader.cpp:46-47, 110-113). The UI layer, outline and ghost
// passes need a few more, looked up by name on demand.
// ─────────────────────────────────────────────────────────────────────────────

impl Shader {
    /// EXT: look up a uniform by name. `None` if the shader does not declare it (or the
    /// compiler optimised it away), in which case the setters below are silent no-ops.
    pub fn uniform(&self, name: &str) -> Option<glow::UniformLocation> {
        unsafe { self.gl.get_uniform_location(self.prog, name) }
    }

    /// EXT: set a `vec4` uniform on the currently bound program.
    pub fn set_vec4(&self, name: &str, v: [f32; 4]) {
        if let Some(loc) = self.uniform(name) {
            unsafe { self.gl.uniform_4_f32(Some(&loc), v[0], v[1], v[2], v[3]) }
        }
    }

    /// EXT: set an `int` uniform (sampler unit) on the currently bound program.
    pub fn set_i32(&self, name: &str, v: i32) {
        if let Some(loc) = self.uniform(name) {
            unsafe { self.gl.uniform_1_i32(Some(&loc), v) }
        }
    }

    /// EXT: set a `float` uniform on the currently bound program.
    pub fn set_f32(&self, name: &str, v: f32) {
        if let Some(loc) = self.uniform(name) {
            unsafe { self.gl.uniform_1_f32(Some(&loc), v) }
        }
    }

    /// EXT: set a `mat4` uniform on the currently bound program. Transposed on upload, like
    /// `set_mvp`, because `Matrix4` is row-major.
    pub fn set_mat4(&self, name: &str, m: &Matrix4) {
        if let Some(loc) = self.uniform(name) {
            unsafe { self.gl.uniform_matrix_4_f32_slice(Some(&loc), true, &m.m) }
        }
    }
}

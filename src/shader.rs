//! Port of Shader.h / Shader.cpp.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use glow::HasContext;

// EXT: typed loader failures and the asset root (see src/app/).
use crate::app::assets;
use crate::app::error::AssetError;
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
    // EXT: by-name uniform locations, resolved once per program. `Object::draw_impl` sets six
    // named uniforms per object per pass, and a glTF model asks for a few more per primitive
    // per pass; each lookup was a CString allocation plus a driver call, and the locations
    // never change after linking. `None` is cached too, so a uniform a shader does not declare
    // costs one miss and then nothing -- "not declared" is the common case for the EXT
    // uniforms on the ported shaders.
    uniforms: RefCell<HashMap<String, Option<glow::UniformLocation>>>,
}

// PORT: `str.find(pat, from)` has no direct std equivalent (was: str.find("\nin ", ix), Shader.cpp:95).
fn find_from(s: &str, pat: &str, from: usize) -> Option<usize> {
    s[from..].find(pat).map(|i| i + from)
}

// PORT: the attribute scan of Shader::LoadShader (Shader.cpp:93-105): every `in` declaration
// at the start of a line names the next attribute slot, in order. EXT: a free function over
// the source text rather than a block inside the GL path, so the scan is testable.
pub fn scrape_attribs(str: &str) -> Result<Vec<String>, String> {
    let mut attribs = Vec::new();
    let bytes = str.as_bytes();
    let mut ix: usize = 0;
    loop {
        ix = match find_from(str, "\nin ", ix) {
            Some(i) => i,
            None => break,
        };
        // PORT: if there is no ';' after "\nin " the C++ walks backwards from npos
        // (undefined behaviour); it is an error here (was: ix = str.find(";", ix);,
        // Shader.cpp:99).
        ix = find_from(str, ";", ix)
            .ok_or_else(|| format!("malformed 'in' declaration at byte {ix}: no ';' after it"))?;
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
    Ok(attribs)
}

impl Shader {
    // EXT: returns the failure instead of panicking on it; `Resources::acquire_shader` is
    // where it becomes fatal (was: Shader::Shader(const char* name), Shader.cpp:8).
    pub fn new(gl: &Rc<glow::Context>, name: &str) -> Result<Shader, AssetError> {
        //Get the file paths
        // EXT: under the resolved asset root rather than the working directory.
        let vert = assets::path(&format!("Shaders/{}.vert", name));
        let frag = assets::path(&format!("Shaders/{}.frag", name));

        // PORT: the attribs member (Shader.h:17) is a local here, see the note on the struct.
        let mut attribs: Vec<String> = Vec::new();

        //Load the shaders from disk
        let vert_id = Self::load_shader(gl, &vert, glow::VERTEX_SHADER, &mut attribs)?;
        let frag_id = Self::load_shader(gl, &frag, glow::FRAGMENT_SHADER, &mut attribs)?;

        unsafe {
            //Create the program
            let prog_id = gl
                .create_program()
                .map_err(|e| AssetError::Gl(format!("glCreateProgram failed for '{}': {}", name, e)))?;
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
                // returns, silently rendering nothing. The log is returned with the error
                // instead, which is far easier to debug (was: std::ofstream fout(std::string(vert) +
                // ".link.log"); ... progId = 0; return;, Shader.cpp:31-43).
                let log = gl.get_program_info_log(prog_id);
                return Err(AssetError::ShaderLink { name: name.to_string(), log });
            }

            //Get global variable locations
            let mvp_id = gl.get_uniform_location(prog_id, "mvp");
            let mv_id = gl.get_uniform_location(prog_id, "mv");

            Ok(Shader {
                prog: prog_id,
                vert: vert_id,
                frag: frag_id,
                mvp_id,
                mv_id,
                gl: Rc::clone(gl),
                uniforms: RefCell::new(HashMap::new()),
            })
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
        fname: &Path,
        ty: u32,
        attribs: &mut Vec<String>,
    ) -> Result<glow::Shader, AssetError> {
        //Read shader source from disk
        // PORT: a failed std::ifstream silently yields an empty source string (which then fails
        // to compile and writes a .log); the io error is returned instead
        // (was: std::ifstream fin(fname); std::stringstream buff; buff << fin.rdbuf();,
        // Shader.cpp:64-68).
        let str = std::fs::read_to_string(fname)
            .map_err(|source| AssetError::Io { path: fname.to_path_buf(), source })?;

        unsafe {
            //Create and compile shader
            let id = gl.create_shader(ty).map_err(|e| {
                AssetError::Gl(format!("glCreateShader failed for '{}': {}", fname.display(), e))
            })?;
            gl.shader_source(id, &str);
            gl.compile_shader(id);

            //Check to make sure there were no errors
            let is_compiled = gl.get_shader_compile_status(id);
            if !is_compiled {
                // PORT: the C++ writes the info log to "<fname>.log" and returns 0, leaving the
                // program to link against a null shader. The log is returned with the error
                // (was: std::ofstream fout(std::string(fname) + ".log"); ... return 0;,
                // Shader.cpp:79-89).
                let log = gl.get_shader_info_log(id);
                return Err(AssetError::ShaderCompile { path: fname.to_path_buf(), log });
            }

            //Save variable bindings
            if ty == glow::VERTEX_SHADER {
                // EXT: the scan is `scrape_attribs`, split out so it can be tested without a
                // context; its one failure is reported like a compile error, which is what it is.
                attribs.extend(scrape_attribs(&str).map_err(|log| AssetError::ShaderCompile {
                    path: fname.to_path_buf(),
                    log,
                })?);
            }

            //Return the shader id
            Ok(id)
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
    /// EXT: look up a uniform by name, memoised per program. `None` if the shader does not
    /// declare it (or the compiler optimised it away), in which case the setters below are
    /// silent no-ops. Misses are cached too, so a by-name miss is one hash probe.
    pub fn uniform(&self, name: &str) -> Option<glow::UniformLocation> {
        if let Some(loc) = self.uniforms.borrow().get(name) {
            return *loc;
        }
        let loc = unsafe { self.gl.get_uniform_location(self.prog, name) };
        self.uniforms.borrow_mut().insert(name.to_string(), loc);
        loc
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

#[cfg(test)]
mod tests {
    use super::scrape_attribs;

    #[test]
    fn attribs_in_declaration_order() {
        let src = "#version 330\nin vec3 in_pos;\nuniform mat4 mvp;\nin vec2 in_uv;\nin vec3 in_normal;\nvoid main() {}\n";
        assert_eq!(scrape_attribs(src).unwrap(), ["in_pos", "in_uv", "in_normal"]);
    }

    #[test]
    fn only_line_initial_in_counts() {
        // "\nin " is the pattern: an `in` mid-line (a parameter qualifier) is not an attribute.
        let src = "#version 330\nvoid f(in vec3 p);\nin vec4 in_col;\n";
        assert_eq!(scrape_attribs(src).unwrap(), ["in_col"]);
    }

    #[test]
    fn no_declarations_is_empty() {
        assert!(scrape_attribs("#version 330\nvoid main() {}\n").unwrap().is_empty());
    }

    #[test]
    fn shipped_texture_shader_declares_three() {
        let src = std::fs::read_to_string(crate::app::assets::path("Shaders/texture.vert")).unwrap();
        assert_eq!(scrape_attribs(&src).unwrap(), ["in_pos", "in_uv", "in_normal"]);
    }

    #[test]
    fn missing_semicolon_is_an_error() {
        let err = scrape_attribs("#version 330\nin vec3 in_pos\nvoid main() {}\n").unwrap_err();
        assert!(err.contains("no ';'"), "{err}");
    }
}

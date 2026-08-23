//! Port of Resources.h / Resources.cpp -- the weak-pointer caches that de-duplicate meshes,
//! shaders and textures across objects and scenes.
//!
//! PORT: the three free functions each own a function-local `static std::unordered_map`
//! (Resources.cpp:5, 17, 29). Rust cannot express a mutable function-local static safely, and
//! the maps additionally need the GL context, so they become three RefCell<HashMap<..>> fields
//! on a `Resources` struct owned by Engine.
//!
//! PORT: the C++ spells the functions "Aquire" (sic). They are `acquire_*` here
//! (was: AquireMesh / AquireShader / AquireTexture, Resources.h:7-9).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

// EXT: a loader's failure ends the process through one sink (src/app/crash.rs), so the
// hundred-odd scene call sites keep their infallible signatures.
use crate::app::crash::fatal;
use crate::mesh::Mesh;
use crate::shader::Shader;
use crate::texture::Texture;

pub struct Resources {
    // PORT: glow needs an explicit context where the C++ used the global GLEW function
    // pointers, so the caches carry one to hand to each new resource (no C++ equivalent).
    gl: Rc<glow::Context>,
    mesh_map: RefCell<HashMap<String, Weak<Mesh>>>,
    shader_map: RefCell<HashMap<String, Weak<Shader>>>,
    texture_map: RefCell<HashMap<String, Weak<Texture>>>,
}

impl Resources {
    pub fn new(gl: &Rc<glow::Context>) -> Resources {
        Resources {
            gl: Rc::clone(gl),
            mesh_map: RefCell::new(HashMap::new()),
            shader_map: RefCell::new(HashMap::new()),
            texture_map: RefCell::new(HashMap::new()),
        }
    }

    // PORT: `const char* name` -> &str; shared_ptr -> Rc
    // (was: std::shared_ptr<Mesh> AquireMesh(const char* name), Resources.cpp:4).
    pub fn acquire_mesh(&self, name: &str) -> Rc<Mesh> {
        let mut map = self.mesh_map.borrow_mut();
        // PORT: `map[std::string(name)]` default-inserts an empty weak_ptr and returns a
        // reference to it; `entry(..).or_insert_with(Weak::new)` is the same thing
        // (was: std::weak_ptr<Mesh>& mesh = map[std::string(name)], Resources.cpp:6).
        let mesh = map.entry(String::from(name)).or_insert_with(Weak::new);
        // PORT: `expired()` + `lock()` collapse into a single `upgrade()`; None means expired
        // (was: if (mesh.expired()) { ... } else { return mesh.lock(); }, Resources.cpp:7-13).
        match mesh.upgrade() {
            None => {
                let new_mesh = Rc::new(Mesh::new(&self.gl, name).unwrap_or_else(|e| fatal(&e)));
                *mesh = Rc::downgrade(&new_mesh);
                new_mesh
            }
            Some(m) => m,
        }
    }

    // PORT: see acquire_mesh (was: std::shared_ptr<Shader> AquireShader(const char* name),
    // Resources.cpp:16).
    pub fn acquire_shader(&self, name: &str) -> Rc<Shader> {
        let mut map = self.shader_map.borrow_mut();
        let shader = map.entry(String::from(name)).or_insert_with(Weak::new);
        match shader.upgrade() {
            None => {
                let new_shader = Rc::new(Shader::new(&self.gl, name).unwrap_or_else(|e| fatal(&e)));
                *shader = Rc::downgrade(&new_shader);
                new_shader
            }
            Some(s) => s,
        }
    }

    // PORT: the C++ default arguments `int rows=1, int cols=1` (Resources.h:9) have no Rust
    // equivalent -- every caller passes them explicitly
    // (was: std::shared_ptr<Texture> AquireTexture(const char* name, int rows, int cols),
    //  Resources.cpp:28).
    //
    // NOTE (original quirk, preserved): the cache key is the file name only, so asking for the
    // same texture with different rows/cols returns the first-built one (Resources.cpp:30).
    pub fn acquire_texture(&self, name: &str, rows: i32, cols: i32) -> Rc<Texture> {
        let mut map = self.texture_map.borrow_mut();
        let tex = map.entry(String::from(name)).or_insert_with(Weak::new);
        match tex.upgrade() {
            None => {
                let new_tex =
                    Rc::new(Texture::new(&self.gl, name, rows, cols).unwrap_or_else(|e| fatal(&e)));
                *tex = Rc::downgrade(&new_tex);
                new_tex
            }
            Some(t) => t,
        }
    }
}

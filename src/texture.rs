//! Port of Texture.h / Texture.cpp.

use std::rc::Rc;

use glow::{HasContext, PixelUnpackData};

pub struct Texture {
    tex: glow::Texture,
    is_3d: bool,
    gl: Rc<glow::Context>,
}

impl Texture {
    pub fn new(gl: &Rc<glow::Context>, fname: &str, rows: i32, cols: i32) -> Texture {
        //Check if this is a 3D texture
        // PORT: C++ `assert` (compiled out in release) -> debug_assert! (was: assert(rows >= 1 &&
        // cols >= 1), Texture.cpp:7).
        debug_assert!(rows >= 1 && cols >= 1);
        let is_3d = rows > 1 || cols > 1;

        //Open the bitmap
        // PORT: the whole file is slurped into a Vec<u8> and walked with a cursor instead of
        // being streamed through an ifstream; the sequential fin.read() calls below become
        // copies from `data` at `pos` (was: std::ifstream fin(..., std::ios::binary),
        // Texture.cpp:11).
        // PORT: on a missing file the C++ sets texId = 0 and returns, silently binding the
        // default texture forever. `tex` is a non-nullable glow::Texture here, so we panic
        // (was: if (!fin) { texId = 0; return; }, Texture.cpp:12-15).
        let path = format!("Textures/{}", fname);
        let data = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("Failed to open texture '{}': {}", path, e));

        //Read the bitmap
        // PORT: `*reinterpret_cast<int32_t*>(&input[18])` -> from_le_bytes; BMP headers are
        // always little-endian, which matched the original's x86 target
        // (was: const GLsizei width = *reinterpret_cast<int32_t*>(&input[18]), Texture.cpp:20-21).
        let input = &data[0..54];
        let width = i32::from_le_bytes([input[18], input[19], input[20], input[21]]);
        let height = i32::from_le_bytes([input[22], input[23], input[24], input[25]]);
        let mut pos: usize = 54;

        // EXT: 32-bit BMP support (BGRA with a real alpha channel), used by the UI atlases --
        // cursor sprites and anti-aliased text need alpha, and a colour key would fringe. The
        // original loader assumes 24-bit (Texture.cpp:18-41); that path is untouched below.
        // A 32-bit file takes this early branch: rows are read bottom-first exactly like the
        // 24-bit path (so GL t=0 lands on the image TOP, same as every other texture here),
        // uploaded as RGBA8 with linear filtering (sprites and glyphs are scaled on screen).
        let bpp = u16::from_le_bytes([input[28], input[29]]);
        if bpp == 32 {
            debug_assert!(!is_3d, "32-bit atlases are never 2D arrays");
            let mut img = vec![0u8; (width * height * 4) as usize];
            for y in (0..height).rev() {
                let ptr = (y * width * 4) as usize;
                img[ptr..ptr + (width * 4) as usize]
                    .copy_from_slice(&data[pos..pos + (width * 4) as usize]);
                pos += (width * 4) as usize; // 4-byte pixels: rows are already aligned
            }
            unsafe {
                let tex = gl
                    .create_texture()
                    .unwrap_or_else(|e| panic!("glGenTextures failed for '{}': {}", path, e));
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                // Trilinear + repeat: 32-bit textures are the "modern" path (UI atlases, the
                // tileable grass noise). Mipmaps matter for the grass -- without them a
                // high-frequency texture shimmers at distance, which reads as cheap instantly.
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width,
                    height,
                    0,
                    glow::BGRA,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(Some(&img)),
                );
                gl.generate_mipmap(glow::TEXTURE_2D);
                return Texture {
                    tex,
                    is_3d: false,
                    gl: Rc::clone(gl),
                };
            }
        }

        // PORT: C++ `assert` -> debug_assert! (was: assert(width % cols == 0), Texture.cpp:22-23).
        debug_assert!(width % cols == 0);
        debug_assert!(height % rows == 0);
        let block_w = width / cols;
        let block_h = height / rows;
        let mut img = vec![0u8; (width * height * 3) as usize];
        //for (int y = height; y--> 0;)
        for y in (0..height).rev() {
            let row = y / block_h;
            let ty = y % block_h;
            for x in 0..width {
                let col = x / block_w;
                let tx = x % block_w;
                let ptr =
                    (((row * cols + col) * (block_w * block_h) + ty * block_w + tx) * 3) as usize;
                img[ptr..ptr + 3].copy_from_slice(&data[pos..pos + 3]);
                pos += 3;
            }
            let padding = (width * 3) % 4;
            if padding != 0 {
                pos += (4 - padding) as usize;
            }
        }

        //Load texture into video memory
        unsafe {
            let tex = gl
                .create_texture()
                .unwrap_or_else(|e| panic!("glGenTextures failed for '{}': {}", path, e));
            if is_3d {
                gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(tex));
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D_ARRAY,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR_MIPMAP_NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D_ARRAY,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D_ARRAY,
                    glow::TEXTURE_WRAP_S,
                    glow::REPEAT as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D_ARRAY,
                    glow::TEXTURE_WRAP_T,
                    glow::REPEAT as i32,
                );
                // PORT: GL_GENERATE_MIPMAP does not exist in a core profile (and the original's
                // own glGenerateMipmap call names the wrong target, GL_TEXTURE_2D, so it is a
                // no-op). Both are dropped and replaced by the explicit
                // generate_mipmap(TEXTURE_2D_ARRAY) after the upload below, which reproduces the
                // Windows compat-profile behaviour of auto-generating mipmaps on upload --
                // required because MIN_FILTER is LINEAR_MIPMAP_NEAREST and a mipmap-less array
                // texture samples black (was: glTexParameteri(GL_TEXTURE_2D_ARRAY,
                // GL_GENERATE_MIPMAP, GL_TRUE); glGenerateMipmap(GL_TEXTURE_2D);,
                // Texture.cpp:51-52).
                // NOTE: `width/rows` and `height/cols` are the original's own inverted operands
                // (block_w is width/cols and block_h is height/rows). Transcribed literally --
                // harmless because the only atlas, floorplan_textures.bmp, is a square 4x4.
                gl.tex_image_3d(
                    glow::TEXTURE_2D_ARRAY,
                    0,
                    glow::RGB8 as i32,
                    width / rows,
                    height / cols,
                    rows * cols,
                    0,
                    glow::BGR,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(Some(&img)),
                );
                gl.generate_mipmap(glow::TEXTURE_2D_ARRAY);
            } else {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGB8 as i32,
                    width,
                    height,
                    0,
                    glow::BGR,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(Some(&img)),
                );
            }

            //Clenup
            // PORT: `delete[] img` is implicit -- `img` is a Vec (was: delete[] img,
            // Texture.cpp:64).

            Texture {
                tex,
                is_3d,
                gl: Rc::clone(gl),
            }
        }
    }

    // PORT: renamed from Use to avoid the `use` keyword (was: void Texture::Use(),
    // Texture.cpp:67).
    pub fn use_texture(&self) {
        if self.is_3d {
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(self.tex));
            }
        } else {
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
            }
        }
    }
}

// PORT: Texture has no destructor in C++, so every texture leaks its GL object when a scene is
// unloaded. Drop deletes it (was: no ~Texture, Texture.h:4-13).
impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_texture(self.tex);
        }
    }
}

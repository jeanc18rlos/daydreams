//! Port of Texture.h / Texture.cpp.

use std::rc::Rc;

use glow::{HasContext, PixelUnpackData};

// EXT: typed loader failures and the asset root (see src/app/).
use crate::app::assets;
use crate::app::error::AssetError;

pub struct Texture {
    tex: glow::Texture,
    is_3d: bool,
    gl: Rc<glow::Context>,
}

// EXT: a decoded bitmap, laid out exactly as the GL upload wants it. `decode_bmp` is the
// byte walk of Texture.cpp:11-41 split away from the GL calls (the same liberty mesh.rs takes
// with `parse_obj`), so a truncated or malformed file is a testable error rather than an
// out-of-bounds slice inside a constructor that needs a context to reach.
#[derive(Debug)]
pub struct Bmp {
    pub width: i32,
    pub height: i32,
    /// 24 (BGR, rearranged into `rows x cols` blocks) or 32 (BGRA, as stored).
    pub bpp: u16,
    pub pixels: Vec<u8>,
}

pub fn decode_bmp(data: &[u8], rows: i32, cols: i32) -> Result<Bmp, String> {
    //Read the bitmap
    // PORT: `*reinterpret_cast<int32_t*>(&input[18])` -> from_le_bytes; BMP headers are
    // always little-endian, which matched the original's x86 target
    // (was: const GLsizei width = *reinterpret_cast<int32_t*>(&input[18]), Texture.cpp:20-21).
    let input = data
        .get(0..54)
        .ok_or_else(|| format!("{} bytes is shorter than a BMP header", data.len()))?;
    if &input[0..2] != b"BM" {
        return Err("missing the 'BM' signature".to_string());
    }
    let width = i32::from_le_bytes([input[18], input[19], input[20], input[21]]);
    let height = i32::from_le_bytes([input[22], input[23], input[24], input[25]]);
    if width <= 0 || height <= 0 {
        // A negative height is a top-down BMP, which the original's bottom-first row walk
        // does not read; nothing the project generates is one.
        return Err(format!("unsupported dimensions {width}x{height}"));
    }
    // PORT: the pixel data is read from byte 54, as the C++ does (Texture.cpp:19-24); the
    // header's own data offset is not consulted, so every BMP here carries a 40-byte info
    // header, which is what the project's generators write.
    let mut pos: usize = 54;

    // EXT: 32-bit BMP support (BGRA with a real alpha channel), used by the UI atlases --
    // cursor sprites and anti-aliased text need alpha, and a colour key would fringe. The
    // original loader assumes 24-bit (Texture.cpp:18-41); that path is untouched below.
    // A 32-bit file takes this early branch: rows are read bottom-first exactly like the
    // 24-bit path (so GL t=0 lands on the image TOP, same as every other texture here).
    let bpp = u16::from_le_bytes([input[28], input[29]]);
    let (w, h) = (width as usize, height as usize);
    // EXT: checked, because the header is untrusted input: a 32-bit width and height
    // multiplied by the pixel size can wrap a usize, and a wrapped `need` would pass the
    // length check and then index past the slice.
    let too_big = || format!("{width}x{height} does not fit in memory");
    match bpp {
        32 => {
            let size = w.checked_mul(h).and_then(|n| n.checked_mul(4)).ok_or_else(too_big)?;
            let need = size.checked_add(54).ok_or_else(too_big)?;
            if data.len() < need {
                return Err(format!(
                    "truncated: {} bytes, {width}x{height}x32 needs {need}",
                    data.len()
                ));
            }
            let mut img = vec![0u8; size];
            for y in (0..h).rev() {
                let ptr = y * w * 4;
                img[ptr..ptr + w * 4].copy_from_slice(&data[pos..pos + w * 4]);
                pos += w * 4; // 4-byte pixels: rows are already aligned
            }
            Ok(Bmp { width, height, bpp, pixels: img })
        }
        24 => {
            let row_bytes = w.checked_mul(3).ok_or_else(too_big)?;
            let padding = row_bytes % 4;
            let stride = row_bytes + if padding != 0 { 4 - padding } else { 0 };
            let need = stride.checked_mul(h).and_then(|n| n.checked_add(54)).ok_or_else(too_big)?;
            if data.len() < need {
                return Err(format!(
                    "truncated: {} bytes, {width}x{height}x24 needs {need}",
                    data.len()
                ));
            }
            // EXT: an error, not an assert, because the atlas shape is data too: `rows` and
            // `cols` are what the scene asked for and the file is what is on disk, and a
            // mismatch between them is a bad asset, not a programming error. The C++ asserted
            // (was: assert(width % cols == 0); assert(height % rows == 0), Texture.cpp:22-23),
            // which in its release build meant a division by zero or a scrambled atlas.
            if rows <= 0 || cols <= 0 {
                return Err(format!("atlas of {rows}x{cols} blocks; both counts must be positive"));
            }
            if width % cols != 0 || height % rows != 0 {
                return Err(format!("{width}x{height} does not divide into {rows}x{cols} blocks"));
            }
            let block_w = width / cols;
            let block_h = height / rows;
            let block_len = (block_w * block_h) as usize;
            // `row_bytes * h` is at most `stride * h`, which fit above.
            let mut img = vec![0u8; row_bytes * h];
            //for (int y = height; y--> 0;)
            for y in (0..height).rev() {
                let row = y / block_h;
                let ty = y % block_h;
                for x in 0..width {
                    let col = x / block_w;
                    let tx = x % block_w;
                    // In usize: the block index times the block area is the pixel count, which
                    // is within the buffer just sized but can be past i32 on a large atlas.
                    let ptr = ((row * cols + col) as usize * block_len
                        + (ty * block_w + tx) as usize)
                        * 3;
                    img[ptr..ptr + 3].copy_from_slice(&data[pos..pos + 3]);
                    pos += 3;
                }
                if padding != 0 {
                    pos += 4 - padding;
                }
            }
            Ok(Bmp { width, height, bpp, pixels: img })
        }
        other => Err(format!("{other} bits per pixel; only 24 and 32 are read")),
    }
}

impl Texture {
    // EXT: returns the failure instead of panicking on it; `Resources::acquire_texture` is
    // where it becomes fatal (was: Texture::Texture(const char* fname, int rows, int cols),
    // Texture.cpp:6).
    pub fn new(
        gl: &Rc<glow::Context>,
        fname: &str,
        rows: i32,
        cols: i32,
    ) -> Result<Texture, AssetError> {
        //Check if this is a 3D texture
        // PORT: C++ `assert` (compiled out in release) -> debug_assert! (was: assert(rows >= 1 &&
        // cols >= 1), Texture.cpp:7).
        debug_assert!(rows >= 1 && cols >= 1);
        let is_3d = rows > 1 || cols > 1;

        //Open the bitmap
        // PORT: the whole file is slurped into a Vec<u8> and walked with a cursor instead of
        // being streamed through an ifstream (was: std::ifstream fin(..., std::ios::binary),
        // Texture.cpp:11).
        // PORT: on a missing file the C++ sets texId = 0 and returns, silently binding the
        // default texture forever. `tex` is a non-nullable glow::Texture here, so the failure
        // is returned (was: if (!fin) { texId = 0; return; }, Texture.cpp:12-15).
        // EXT: under the resolved asset root rather than the working directory.
        let path = assets::path(&format!("Textures/{}", fname));
        let data =
            std::fs::read(&path).map_err(|source| AssetError::Io { path: path.clone(), source })?;
        let Bmp { width, height, bpp, pixels: img } = decode_bmp(&data, rows, cols)
            .map_err(|reason| AssetError::BadBmp { path: path.clone(), reason })?;
        let gl_error = |e: String| {
            AssetError::Gl(format!("glGenTextures failed for '{}': {}", path.display(), e))
        };

        if bpp == 32 {
            debug_assert!(!is_3d, "32-bit atlases are never 2D arrays");
            // Uploaded as RGBA8 with linear filtering (sprites and glyphs are scaled on screen).
            unsafe {
                let tex = gl.create_texture().map_err(gl_error)?;
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                // Trilinear + repeat: 32-bit textures are the "modern" path (UI atlases, the
                // tileable grass noise). Mipmaps matter for the grass -- without them a
                // high-frequency texture shimmers at distance, which reads as cheap instantly.
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR_MIPMAP_LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
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
                return Ok(Texture { tex, is_3d: false, gl: Rc::clone(gl) });
            }
        }

        //Load texture into video memory
        unsafe {
            let tex = gl.create_texture().map_err(gl_error)?;
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

            Ok(Texture { tex, is_3d, gl: Rc::clone(gl) })
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

#[cfg(test)]
mod tests {
    use super::decode_bmp;

    /// A minimal BMP: 54-byte header, then rows bottom-first.
    fn bmp(width: i32, height: i32, bpp: u16, rows: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0u8; 54];
        out[0] = b'B';
        out[1] = b'M';
        out[18..22].copy_from_slice(&width.to_le_bytes());
        out[22..26].copy_from_slice(&height.to_le_bytes());
        out[28..30].copy_from_slice(&bpp.to_le_bytes());
        for row in rows {
            out.extend_from_slice(row);
        }
        out
    }

    #[test]
    fn decodes_24_bit_with_row_padding() {
        // 1x2 pixels: a 3-byte row is padded to 4. File order is bottom row first.
        let bottom = vec![1, 2, 3, 0];
        let top = vec![4, 5, 6, 0];
        let b = decode_bmp(&bmp(1, 2, 24, &[bottom, top]), 1, 1).unwrap();
        assert_eq!((b.width, b.height, b.bpp), (1, 2, 24));
        // Bottom-first in the file lands at the END of the buffer (Texture.cpp:25 walks y down).
        assert_eq!(b.pixels, [4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn decodes_24_bit_into_atlas_blocks() {
        // 2x2 pixels as a 1-row, 2-column atlas: the two columns become two consecutive
        // 1x2 blocks rather than two interleaved rows.
        let bottom = vec![10, 10, 10, 20, 20, 20, 0, 0];
        let top = vec![30, 30, 30, 40, 40, 40, 0, 0];
        let b = decode_bmp(&bmp(2, 2, 24, &[bottom, top]), 1, 2).unwrap();
        assert_eq!(b.pixels, [30, 30, 30, 10, 10, 10, 40, 40, 40, 20, 20, 20]);
    }

    #[test]
    fn decodes_32_bit() {
        let bottom = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let top = vec![9, 10, 11, 12, 13, 14, 15, 16];
        let b = decode_bmp(&bmp(2, 2, 32, &[bottom, top]), 1, 1).unwrap();
        assert_eq!(b.bpp, 32);
        assert_eq!(b.pixels, [9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn truncated_files_are_errors() {
        let whole = bmp(2, 2, 24, &[vec![0; 8], vec![0; 8]]);
        assert!(decode_bmp(&whole, 1, 1).is_ok());
        let err = decode_bmp(&whole[..whole.len() - 1], 1, 1).unwrap_err();
        assert!(err.contains("truncated"), "{err}");
        let err = decode_bmp(&whole[..20], 1, 1).unwrap_err();
        assert!(err.contains("header"), "{err}");
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(decode_bmp(&bmp(1, 1, 8, &[vec![0; 4]]), 1, 1)
            .unwrap_err()
            .contains("bits per pixel"));
        assert!(decode_bmp(&bmp(1, -1, 24, &[vec![0; 4]]), 1, 1)
            .unwrap_err()
            .contains("dimensions"));
        let mut bad = bmp(1, 1, 24, &[vec![0; 4]]);
        bad[0] = b'X';
        assert!(decode_bmp(&bad, 1, 1).unwrap_err().contains("signature"));
    }

    #[test]
    fn atlas_shape_must_divide_the_image() {
        let two_by_two = bmp(2, 2, 24, &[vec![0; 8], vec![0; 8]]);
        assert!(decode_bmp(&two_by_two, 2, 2).is_ok());
        assert!(decode_bmp(&two_by_two, 2, 3).unwrap_err().contains("does not divide"));
        assert!(decode_bmp(&two_by_two, 3, 1).unwrap_err().contains("does not divide"));
        assert!(decode_bmp(&two_by_two, 0, 1).unwrap_err().contains("positive"));
        assert!(decode_bmp(&two_by_two, 1, -1).unwrap_err().contains("positive"));
        // The 32-bit path takes no atlas shape, so the counts are not checked there.
        let rgba = bmp(1, 1, 32, &[vec![0; 4]]);
        assert!(decode_bmp(&rgba, 0, 0).is_ok());
    }

    #[test]
    fn absurd_dimensions_are_errors_not_overflows() {
        // A header claiming i32::MAX on both axes. The byte count is just under 2^64, so a
        // 64-bit usize holds it and the error is "truncated"; a 32-bit target wraps and gets
        // "fit in memory". Either way it is an Err and not a wrapped `need` that passes the
        // length check and lets the pixel walk index past the slice.
        for bpp in [24u16, 32] {
            let huge = bmp(i32::MAX, i32::MAX, bpp, &[]);
            let err = decode_bmp(&huge, 1, 1).unwrap_err();
            assert!(err.contains("truncated") || err.contains("fit in memory"), "{err}");
        }
    }

    #[test]
    fn shipped_atlas_decodes() {
        let data =
            std::fs::read(crate::app::assets::path("Textures/floorplan_textures.bmp")).unwrap();
        let b = decode_bmp(&data, 4, 4).unwrap();
        assert_eq!(b.bpp, 24);
        assert_eq!(b.pixels.len(), (b.width * b.height * 3) as usize);
    }
}

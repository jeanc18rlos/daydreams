//! Port of FrameBuffer.h / FrameBuffer.cpp -- the off-screen render target a Portal draws
//! its recursive view into.
//!
//! EXT: the C++ gives every Portal its own `GH_FBO_SIZE`-square FrameBuffer per recursion
//! level (Portal.h:43). Here the framebuffers are owned by `Engine`, one per recursion level
//! for ALL portals, and sized to the drawable; see `Engine::portal_fbos` for why that is
//! equivalent. This file only gains a size.

use std::rc::Rc;

use glow::HasContext;

use crate::camera::Camera;
use crate::object::RenderCtx;

pub struct FrameBuffer {
    // PORT: GLuint handles become glow's opaque handle types (was: GLuint texId; GLuint fbo;
    // GLuint renderBuf, FrameBuffer.h:16-18).
    tex_id: glow::Texture,
    fbo: glow::Framebuffer,
    render_buf: glow::Renderbuffer,
    // PORT: the C++ reaches the GL through the global GLEW function pointers; glow needs an
    // explicit context, so every GL-owning type carries an Rc to it (no C++ equivalent).
    gl: Rc<glow::Context>,
    // EXT: the attachment size, where the C++ hard-codes GH_FBO_SIZE square.
    pub width: i32,
    pub height: i32,
}

impl FrameBuffer {
    // PORT: the default constructor takes the GL context as a parameter
    // (was: FrameBuffer::FrameBuffer(), FrameBuffer.cpp:6).
    // EXT: and the size of the attachments, which the C++ fixes at GH_FBO_SIZE square.
    pub fn new(gl: &Rc<glow::Context>, width: i32, height: i32) -> FrameBuffer {
        unsafe {
            // PORT: glGenTextures(1, &texId) -> create_texture(), which returns a Result
            // (was: glGenTextures(1, &texId), FrameBuffer.cpp:7).
            let tex_id = gl.create_texture().expect("glGenTextures failed");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex_id));
            // PORT: GL_CLAMP does not exist in core profile -> GL_CLAMP_TO_EDGE
            // (was: glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP), FrameBuffer.cpp:9).
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            // PORT: GL_CLAMP -> GL_CLAMP_TO_EDGE
            // (was: glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP), FrameBuffer.cpp:10).
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
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
            // PORT: the unsized internal format GL_RGB is not a legal color-attachment format
            // in core profile -> the sized GL_RGB8. The external format stays GL_RGB. nullptr
            // pixel data -> PixelUnpackData::Slice(None)
            // (was: glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB, GH_FBO_SIZE, GH_FBO_SIZE, 0, GL_RGB,
            //  GL_UNSIGNED_BYTE, nullptr), FrameBuffer.cpp:13).
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGB8 as i32,
                width,
                height,
                0,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            //-------------------------
            // PORT: the *EXT framebuffer entry points and GL_FRAMEBUFFER_EXT /
            // GL_COLOR_ATTACHMENT0_EXT enums become their core equivalents
            // (was: glGenFramebuffersEXT(1, &fbo), FrameBuffer.cpp:15).
            let fbo = gl.create_framebuffer().expect("glGenFramebuffers failed");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex_id),
                0,
            );
            //-------------------------
            // PORT: *EXT renderbuffer entry points / GL_RENDERBUFFER_EXT -> core
            // (was: glGenRenderbuffersEXT(1, &renderBuf), FrameBuffer.cpp:19).
            let render_buf = gl
                .create_renderbuffer()
                .expect("glGenRenderbuffers failed");
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(render_buf));
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::DEPTH_COMPONENT16,
                width,
                height,
            );
            //-------------------------
            // PORT: GL_DEPTH_ATTACHMENT_EXT -> GL_DEPTH_ATTACHMENT
            // (was: glFramebufferRenderbufferEXT(GL_FRAMEBUFFER_EXT, GL_DEPTH_ATTACHMENT_EXT,
            //  GL_RENDERBUFFER_EXT, renderBuf), FrameBuffer.cpp:23).
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(render_buf),
            );
            //-------------------------

            //Does the GPU support current FBO configuration?
            // PORT: glCheckFramebufferStatusEXT -> check_framebuffer_status
            // (was: GLenum status = glCheckFramebufferStatusEXT(GL_FRAMEBUFFER_EXT), FrameBuffer.cpp:27).
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                // PORT: the C++ returns silently here, leaving the incomplete FBO bound and
                // the object half-constructed; we keep that control flow but print a warning
                // (was: if (status != GL_FRAMEBUFFER_COMPLETE_EXT) { return; }, FrameBuffer.cpp:28-30).
                eprintln!("Framebuffer is not complete: 0x{:X}", status);
                return FrameBuffer {
                    tex_id,
                    fbo,
                    render_buf,
                    gl: Rc::clone(gl),
                    width,
                    height,
                };
            }

            //Unbind so future rendering can proceed normally
            // PORT: framebuffer name 0 (the default framebuffer) is None in glow
            // (was: glBindFramebufferEXT(GL_FRAMEBUFFER_EXT, 0), FrameBuffer.cpp:33).
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);

            FrameBuffer {
                tex_id,
                fbo,
                render_buf,
                gl: Rc::clone(gl),
                width,
                height,
            }
        }
    }

    // PORT: renamed from Use() -- `use` is a Rust keyword
    // (was: void FrameBuffer::Use(), FrameBuffer.cpp:36).
    pub fn use_texture(&self) {
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_id));
        }
    }

    // PORT: the global GH_ENGINE is threaded in as `ctx` instead, and `GLuint curFBO` becomes
    // Option<glow::Framebuffer> where None is the default framebuffer (GLuint 0). The
    // `const Portal* skipPortal` identity pointer becomes the portal's u32 id
    // (was: void FrameBuffer::Render(const Camera& cam, GLuint curFBO, const Portal* skipPortal),
    //  FrameBuffer.cpp:40).
    pub fn render(
        &self,
        ctx: &RenderCtx,
        cam: &Camera,
        cur_fbo: Option<glow::Framebuffer>,
        skip_portal: Option<u32>,
    ) {
        unsafe {
            ctx.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            // EXT: the attachment size (was: glViewport(0, 0, GH_FBO_SIZE, GH_FBO_SIZE),
            // FrameBuffer.cpp:42).
            ctx.gl.viewport(0, 0, self.width, self.height);
            ctx.engine.render(cam, Some(self.fbo), skip_portal);
            ctx.gl.bind_framebuffer(glow::FRAMEBUFFER, cur_fbo);
        }
    }
}

// PORT: FrameBuffer has no destructor at all in C++, so every GL object it owns leaks when a
// scene is unloaded -- cycling the 7 scenes leaks ~78 framebuffers, roughly 1.8 GiB of texture
// and renderbuffer memory. Drop deletes them (no C++ equivalent, FrameBuffer.h:8-19).
// EXT: with the engine owning the three shared framebuffers there is nothing left to leak per
// scene; Drop now runs on a window resize and at shutdown.
impl Drop for FrameBuffer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_texture(self.tex_id);
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_renderbuffer(self.render_buf);
        }
    }
}

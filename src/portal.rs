//! Port of Portal.h / Portal.cpp.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::camera::Camera;
use crate::game_header::{gh_clamp, gh_min};
use crate::object::{Object, ObjectT, RenderCtx};
use crate::resources::Resources;
use crate::shader::Shader;
use crate::vector::{Matrix4, Vector3, Vector4};

// PORT: the C++ Warp stores `const Portal* fromPortal / toPortal`, raw pointers that are used
// ONLY for identity comparison (`vPortals[i].get() != skipPortal`, Engine.cpp:237/244/253) and
// to reach the owning portal's transforms inside Connect. Rust cannot hand out a raw self
// pointer from a constructor and later compare it through an Rc, so every portal gets a unique
// u32 id from this counter and the Warp stores ids instead
// (was: Portal(const Portal* fromPortal), Portal.h:14-22).
static NEXT_PORTAL_ID: AtomicU32 = AtomicU32::new(0);

/// PORT: new type with no C++ counterpart. `Portal::Connect(Warp& a, Warp& b)` (Portal.cpp:111)
/// is called as `Connect(p1->front, p3->back)` -- a mutable reference to a member of another
/// object. Borrowck forbids that when the portals live in `Rc<RefCell<..>>`, so the caller names
/// the side with this enum instead and `connect_warps` does the borrowing.
#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Front,
    Back,
}

/// Subclass that represents a warp
#[derive(Clone, Copy)]
pub struct Warp {
    pub delta: Matrix4,
    pub delta_inv: Matrix4,
    // Only read via Warp::new/Connect; the recursion guard compares `to_portal`.
    #[allow(dead_code)]
    pub from_portal: u32,
    // PORT: `const Portal* toPortal` starts as nullptr -> Option<u32> (was: Portal.h:22).
    pub to_portal: Option<u32>,
}

impl Warp {
    fn new(from_portal: u32) -> Warp {
        Warp {
            // PORT: the C++ ctor default-constructs the matrices (leaving them uninitialized)
            // and then calls MakeIdentity() on both; Matrix4::identity() is the same value
            // (was: delta.MakeIdentity(); deltaInv.MakeIdentity();, Portal.h:15-16).
            delta: Matrix4::identity(),
            delta_inv: Matrix4::identity(),
            from_portal,
            to_portal: None,
        }
    }
}

pub struct Portal {
    // PORT: `class Portal : public Object` -> composition. Rust has no inheritance; the base
    // subobject becomes a field and the virtual interface becomes the ObjectT trait
    // (was: Portal.h:10).
    pub base: Object,
    /// PORT: identity token replacing the raw `this` pointer stored in Warp::fromPortal.
    pub id: u32,
    pub front: Warp,
    pub back: Warp,
    err_shader: Rc<Shader>,
    // EXT: `FrameBuffer frameBuf[GH_MAX_RECURSION <= 1 ? 1 : GH_MAX_RECURSION - 1]`
    // (Portal.h:43) is not a member any more. The engine owns one framebuffer per recursion
    // level for every portal (`Engine::portal_fbos`) and `draw` borrows it through the
    // RenderCtx.
}

impl Portal {
    // PORT: the ctor takes `gl` and `res`. C++ reaches the resource caches through the
    // file-scope AquireXxx() functions (was: Portal::Portal() : front(this), back(this),
    // Portal.cpp:6). `gl` is no longer read -- the FrameBuffers it constructed live on the
    // engine now -- and is kept so the scene code's call sites stay as they were.
    pub fn new(_gl: &Rc<glow::Context>, res: &Resources) -> Portal {
        let id = NEXT_PORTAL_ID.fetch_add(1, Ordering::Relaxed);

        let mut base = Object::new();
        base.mesh = Some(res.acquire_mesh("double_quad.obj"));
        base.shader = Some(res.acquire_shader("portal"));

        Portal {
            base,
            id,
            front: Warp::new(id),
            back: Warp::new(id),
            err_shader: res.acquire_shader("pink"),
        }
    }

    pub fn draw(&self, ctx: &RenderCtx, cam: &Camera, cur_fbo: Option<glow::Framebuffer>) {
        // PORT: C's assert() is debug-only, so debug_assert! is the exact equivalent
        // (was: assert(euler.x == 0.0f), Portal.cpp:13-14).
        debug_assert!(self.base.euler.x == 0.0);
        debug_assert!(self.base.euler.z == 0.0);

        //Draw pink to indicate end of render chain
        // PORT: GH_REC_LEVEL is a Cell field on Engine, read through the RenderCtx
        // (was: if (GH_REC_LEVEL <= 0), Portal.cpp:17).
        if ctx.engine.rec_level() <= 0 {
            self.draw_pink(ctx, cam);
            return;
        }

        //Find normal relative to camera
        let mut normal = self.base.forward();
        let cam_pos = cam.world_view.inverse().translation();
        let front_direction = (cam_pos - self.base.pos).dot(normal) > 0.0;
        let warp = if front_direction {
            &self.front
        } else {
            &self.back
        };
        if front_direction {
            normal = -normal;
        }

        //Extra clipping to prevent artifacts
        // PORT: GH_ENGINE global -> ctx.engine (was: GH_ENGINE->NearestPortalDist(), Portal.cpp:32).
        let extra_clip = gh_min(ctx.engine.nearest_portal_dist() * 0.5, 0.1);

        //Create new portal camera
        // EXT: the engine's framebuffer for this recursion level (was: frameBuf[GH_REC_LEVEL - 1]
        // on this portal, Portal.cpp:42), and its size in place of GH_FBO_SIZE square.
        let rec_level = ctx.engine.rec_level();
        let frame_buf = &ctx.portal_fbos[(rec_level - 1) as usize];
        let mut portal_cam = *cam;
        portal_cam.clip_oblique(self.base.pos - normal * extra_clip, -normal);
        portal_cam.world_view = portal_cam.world_view * warp.delta;
        portal_cam.width = frame_buf.width;
        portal_cam.height = frame_buf.height;

        //Render portal's view from new camera
        // EXT: only the quad's screen footprint of that framebuffer is ever sampled (see the
        // projective lookup in Shaders/portal.frag), so the nested pass is scissored to it.
        // The box is in the nested framebuffer's pixels: its viewport stretches this pass's
        // projection over the whole texture, so NDC maps straight to it. See src/ext/scissor.rs.
        let clip = crate::ext::scissor::quad_clip(&self.base.local_to_world(), &cam.matrix());
        let scissor = crate::ext::scissor::ScissorGuard::begin(
            ctx.gl,
            crate::ext::scissor::footprint(&clip, frame_buf.width, frame_buf.height),
        );
        frame_buf.render(ctx, &portal_cam, cur_fbo, warp.to_portal);
        scissor.end(ctx.gl);
        cam.use_viewport(ctx.gl);

        //Now we can render the portal texture to the screen
        let mv = self.base.local_to_world();
        let mvp = cam.matrix() * mv;
        // PORT: `mesh` and `shader` are Option<Rc<..>> on the ported Object, while C++
        // dereferences the shared_ptrs unconditionally. Portal::new always fills both, so
        // unwrap reproduces the C++ behaviour exactly (was: shader->Use(), Portal.cpp:48).
        let shader = self.base.shader.as_ref().unwrap();
        let mesh = self.base.mesh.as_ref().unwrap();
        shader.use_program();
        frame_buf.use_texture();
        shader.set_mvp(Some(&mvp), Some(&mv));
        mesh.draw();
    }

    // PORT: takes `ctx` only so DrawPink matches the call sites that already hold one; C++
    // needs nothing extra because Use()/Draw() reach the GL context implicitly
    // (was: void Portal::DrawPink(const Camera& cam), Portal.cpp:54).
    pub fn draw_pink(&self, _ctx: &RenderCtx, cam: &Camera) {
        let mv = self.base.local_to_world();
        let mvp = cam.matrix() * mv;
        let mesh = self.base.mesh.as_ref().unwrap();
        self.err_shader.use_program();
        self.err_shader.set_mvp(Some(&mvp), Some(&mv));
        mesh.draw();
    }

    pub fn get_bump(&self, a: Vector3) -> Vector3 {
        let n = self.base.forward();
        n * (if (a - self.base.pos).dot(n) > 0.0 {
            1.0
        } else {
            -1.0
        })
    }

    // PORT: `const Warp*` -> Option<&Warp>; nullptr becomes None (was: Portal.cpp:67).
    pub fn intersects(&self, a: Vector3, b: Vector3, bump: Vector3) -> Option<&Warp> {
        let n = self.base.forward();
        let p = self.base.pos + bump;
        let da = n.dot(a - p);
        let db = n.dot(b - p);
        if da * db > 0.0 {
            return None;
        }
        let m = self.base.local_to_world();
        let d = a + (b - a) * (da / (da - db)) - p;
        let x = (m * Vector4::new(1.0, 0.0, 0.0, 0.0)).xyz();
        if d.dot(x).abs() >= x.dot(x) {
            return None;
        }
        let y = (m * Vector4::new(0.0, 1.0, 0.0, 0.0)).xyz();
        if d.dot(y).abs() >= y.dot(y) {
            return None;
        }
        Some(if da > 0.0 { &self.front } else { &self.back })
    }

    pub fn dist_to(&self, pt: Vector3) -> f32 {
        //Get world delta
        let local_to_world = self.base.local_to_world();
        let v = pt - local_to_world.translation();

        //Get axes
        let x = local_to_world.x_axis();
        let y = local_to_world.y_axis();

        //Find closest point
        let px = gh_clamp(v.dot(x) / x.mag_sq(), -1.0, 1.0);
        let py = gh_clamp(v.dot(y) / y.mag_sq(), -1.0, 1.0);
        let closest = x * px + y * py;

        //Calculate distance to closest point
        (v - closest).mag()
    }

    // PORT: helper with no C++ counterpart; picks the Warp named by a Side. Needed because
    // `Connect(Warp&, Warp&)` cannot be expressed with references through RefCell.
    fn warp_mut(&mut self, side: Side) -> &mut Warp {
        match side {
            Side::Front => &mut self.front,
            Side::Back => &mut self.back,
        }
    }
}

impl ObjectT for Portal {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }
    fn draw(&self, ctx: &RenderCtx, cam: &Camera, cur_fbo: Option<glow::Framebuffer>) {
        Portal::draw(self, ctx, cam, cur_fbo)
    }
}

// PORT: `static void Portal::Connect(std::shared_ptr<Portal>& a, std::shared_ptr<Portal>& b)`
// -> a free function. Rust associated functions cannot take `&Rc<RefCell<Self>>` receivers
// naturally, and the levels call it as a namespaced free function anyway (was: Portal.cpp:106).
pub fn connect(a: &Rc<RefCell<Portal>>, b: &Rc<RefCell<Portal>>) {
    connect_warps(a, Side::Front, b, Side::Back);
    connect_warps(b, Side::Front, a, Side::Back);
}

// PORT: `static void Portal::Connect(Warp& a, Warp& b)` -> free function taking the two owning
// portals plus the side of each warp, because the C++ signature's two mutable Warp references
// would be two simultaneous borrow_mut()s (was: Portal.cpp:111).
pub fn connect_warps(
    a: &Rc<RefCell<Portal>>,
    a_side: Side,
    b: &Rc<RefCell<Portal>>,
    b_side: Side,
) {
    // PORT: both transforms are read (and the borrows dropped) before anything is written.
    // C++ interleaves the reads and writes freely; `a` and `b` may be the same cell, so the
    // borrows must not overlap (was: Portal.cpp:112-117).
    let (a_id, a_local_to_world, a_world_to_local) = {
        let p = a.borrow();
        (
            p.id,
            p.base.local_to_world(),
            p.base.world_to_local(),
        )
    };
    let (b_id, b_local_to_world, b_world_to_local) = {
        let p = b.borrow();
        (
            p.id,
            p.base.local_to_world(),
            p.base.world_to_local(),
        )
    };

    let a_delta = a_local_to_world * b_world_to_local;
    let b_delta = b_local_to_world * a_world_to_local;

    {
        let mut pa = a.borrow_mut();
        let wa = pa.warp_mut(a_side);
        wa.to_portal = Some(b_id);
        wa.delta = a_delta;
        wa.delta_inv = b_delta;
    }
    {
        let mut pb = b.borrow_mut();
        let wb = pb.warp_mut(b_side);
        wb.to_portal = Some(a_id);
        wb.delta = b_delta;
        wb.delta_inv = a_delta;
    }
}

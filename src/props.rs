//! Port of the header-only prop classes: Ground.h, House.h, Pillar.h, PillarRoom.h, Statue.h,
//! Sky.h, Tunnel.h and Floorplan.h.
//!
//! PORT: every one of these is a `class X : public Object` whose entire body is a constructor
//! (plus a couple of const helper methods). Rust has no inheritance, so each class becomes a
//! free function that builds and returns a plain `Object` with the same mesh/shader/texture/scale,
//! and each `SetDoorN`/`SetPortal`/`AddPortals` method becomes a free function taking the prop as
//! its first argument. Tunnel is the one exception: it keeps a struct because SetDoor2 branches
//! on its stored `type` (Tunnel.h:32-41). Sky was never an Object and stays a struct.
//!
//! PORT: every constructor takes `gl` and `res`. C++ reaches the caches through the file-scope
//! AquireMesh/AquireShader/AquireTexture functions (Resources.h:7-9), which are not ported as
//! globals. `AquireTexture`'s defaulted `rows=1, cols=1` arguments are passed explicitly because
//! Rust has no default arguments.

use std::cell::RefCell;
use std::rc::Rc;

use glow::HasContext;

use crate::camera::Camera;
use crate::game_header::GH_PI;
use crate::mesh::Mesh;
use crate::object::{Object, ObjectT};
use crate::portal::{connect_warps, Portal, Side};
use crate::resources::Resources;
use crate::scene::PPortalVec;
use crate::shader::Shader;
use crate::vector::Vector3;

// ---------------------------------------------------------------------------------------------
// Ground.h
// ---------------------------------------------------------------------------------------------

pub fn ground(_gl: &Rc<glow::Context>, res: &Resources, slope: bool) -> Object {
    let mut o = Object::new();
    if slope {
        o.mesh = Some(res.acquire_mesh("ground_slope.obj"));
    } else {
        o.mesh = Some(res.acquire_mesh("ground.obj"));
    }
    // EXT: the ported ground was a 20x20 green checkerboard quad (Ground.h:8-16) floating in
    // the sky gradient. Every outdoor scene now gets the meadow's grass material instead, and
    // the FLAT ground is scaled out to the horizon so distance haze, not a hard edge, ends the
    // world. Scenes still multiply this base scale as they always did (Level5.cpp:12 etc.),
    // which is why the slope keeps the original 10: a 40x slope would be a mountain.
    // (was: shader "texture", texture "checker_green.bmp", scale (10,1,10)).
    o.shader = Some(res.acquire_shader("grass_flat"));
    o.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
    o.scale = if slope {
        Vector3::new(10.0, 1.0, 10.0)
    } else {
        Vector3::new(400.0, 1.0, 400.0)
    };
    o
}

// ---------------------------------------------------------------------------------------------
// House.h
// ---------------------------------------------------------------------------------------------

pub fn house(_gl: &Rc<glow::Context>, res: &Resources, tex: &str) -> Object {
    let mut o = Object::new();
    o.mesh = Some(res.acquire_mesh("square_rooms.obj"));
    o.shader = Some(res.acquire_shader("texture"));
    o.texture = Some(res.acquire_texture(tex, 1, 1));
    o.scale = Vector3::new(1.0, 3.0, 1.0);
    o
}

// PORT: the C++ helpers take `Object& portal` (the base class) even though every call site
// passes a Portal; the contract types them as `&mut Portal`, so the fields are reached through
// `portal.base` (was: void SetDoor1(Object& portal) const, House.h:15).
pub fn house_set_door1(house: &Object, portal: &mut Portal) {
    portal.base.pos = house
        .local_to_world()
        .mul_point(Vector3::new(4.0, 0.5, 10.0));
    portal.base.euler = house.euler;
    portal.base.scale = Vector3::new(2.0, 0.5, 1.0) * house.scale;
}

pub fn house_set_door2(house: &Object, portal: &mut Portal) {
    portal.base.pos = house
        .local_to_world()
        .mul_point(Vector3::new(10.0, 0.5, 4.0));
    portal.base.euler = house.euler;
    portal.base.euler.y -= GH_PI / 2.0;
    portal.base.scale = Vector3::new(2.0, 0.5, 1.0) * house.scale;
}

pub fn house_set_door3(house: &Object, portal: &mut Portal) {
    portal.base.pos = house
        .local_to_world()
        .mul_point(Vector3::new(16.0, 0.5, 10.0));
    portal.base.euler = house.euler;
    portal.base.euler.y -= GH_PI;
    portal.base.scale = Vector3::new(2.0, 0.5, 1.0) * house.scale;
}

pub fn house_set_door4(house: &Object, portal: &mut Portal) {
    portal.base.pos = house
        .local_to_world()
        .mul_point(Vector3::new(10.0, 0.5, 16.0));
    portal.base.euler = house.euler;
    portal.base.euler.y -= GH_PI * 3.0 / 2.0;
    portal.base.scale = Vector3::new(2.0, 0.5, 1.0) * house.scale;
}

// ---------------------------------------------------------------------------------------------
// Pillar.h
// ---------------------------------------------------------------------------------------------

pub fn pillar(_gl: &Rc<glow::Context>, res: &Resources) -> Object {
    let mut o = Object::new();
    o.mesh = Some(res.acquire_mesh("pillar.obj"));
    o.shader = Some(res.acquire_shader("texture"));
    o.texture = Some(res.acquire_texture("white.bmp", 1, 1));
    o.scale = Vector3::splat(0.1);
    o
}

// ---------------------------------------------------------------------------------------------
// PillarRoom.h
// ---------------------------------------------------------------------------------------------

pub fn pillar_room(_gl: &Rc<glow::Context>, res: &Resources) -> Object {
    let mut o = Object::new();
    o.mesh = Some(res.acquire_mesh("pillar_room.obj"));
    o.shader = Some(res.acquire_shader("texture"));
    o.texture = Some(res.acquire_texture("three_room.bmp", 1, 1));
    o.scale = Vector3::splat(1.1);
    o
}

pub fn pillar_room_set_portal(room: &Object, portal: &mut Portal) {
    portal.base.pos = room.local_to_world().mul_point(Vector3::new(0.0, 1.5, -1.0));
    portal.base.euler = room.euler;
    portal.base.euler.y -= GH_PI / 2.0;
    portal.base.scale = Vector3::new(1.0, 1.5, 1.0) * room.scale;
}

// ---------------------------------------------------------------------------------------------
// Statue.h
// ---------------------------------------------------------------------------------------------

pub fn statue(_gl: &Rc<glow::Context>, res: &Resources, model: &str) -> Object {
    let mut o = Object::new();
    o.mesh = Some(res.acquire_mesh(model));
    o.shader = Some(res.acquire_shader("texture"));
    o.texture = Some(res.acquire_texture("gold.bmp", 1, 1));
    o
}

// ---------------------------------------------------------------------------------------------
// Tunnel.h
// ---------------------------------------------------------------------------------------------

// PORT: `enum Type { NORMAL = 0, SCALE = 1, SLOPE = 2 }` -> a Rust enum. The explicit
// discriminants are never used numerically (was: Tunnel.h:6-10).
#[derive(Clone, Copy, PartialEq)]
pub enum TunnelType {
    Normal,
    Scale,
    Slope,
}

pub struct Tunnel {
    // PORT: `class Tunnel : public Object` -> composition (was: Tunnel.h:4).
    pub base: Object,
    // PORT: the member is named `type` in C++, which is a Rust keyword (was: Type type, Tunnel.h:45).
    pub ttype: TunnelType,
}

impl ObjectT for Tunnel {
    fn base(&self) -> &Object {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Object {
        &mut self.base
    }
}

pub fn tunnel(_gl: &Rc<glow::Context>, res: &Resources, t: TunnelType) -> Tunnel {
    let mut base = Object::new();
    if t == TunnelType::Scale {
        base.mesh = Some(res.acquire_mesh("tunnel_scale.obj"));
    } else if t == TunnelType::Slope {
        base.mesh = Some(res.acquire_mesh("tunnel_slope.obj"));
    } else {
        base.mesh = Some(res.acquire_mesh("tunnel.obj"));
    }
    // EXT: the corridor geometry is untouched -- it IS the impossible-length illusion
    // (Level1.cpp) -- but it wears the meadow's grass material, so each tunnel reads as a
    // grassy embankment with a passage cut through it rather than a checkerboard box.
    // (was: shader "texture", texture "checker_gray.bmp", Tunnel.h:19-20).
    base.shader = Some(res.acquire_shader("grass_flat"));
    base.texture = Some(res.acquire_texture("grass_noise.bmp", 1, 1));
    Tunnel { base, ttype: t }
}

pub fn tunnel_set_door1(t: &Tunnel, portal: &mut Portal) {
    portal.base.pos = t
        .base
        .local_to_world()
        .mul_point(Vector3::new(0.0, 1.0, 1.0));
    portal.base.euler = t.base.euler;
    portal.base.scale = Vector3::new(0.6, 0.999, 1.0) * t.base.scale.x;
}

pub fn tunnel_set_door2(t: &Tunnel, portal: &mut Portal) {
    portal.base.euler = t.base.euler;
    if t.ttype == TunnelType::Scale {
        portal.base.pos = t
            .base
            .local_to_world()
            .mul_point(Vector3::new(0.0, 0.5, -1.0));
        portal.base.scale = Vector3::new(0.3, 0.499, 0.5) * t.base.scale.x;
    } else if t.ttype == TunnelType::Slope {
        portal.base.pos = t
            .base
            .local_to_world()
            .mul_point(Vector3::new(0.0, -1.0, -1.0));
        portal.base.scale = Vector3::new(0.6, 0.999, 1.0) * t.base.scale.x;
    } else {
        portal.base.pos = t
            .base
            .local_to_world()
            .mul_point(Vector3::new(0.0, 1.0, -1.0));
        portal.base.scale = Vector3::new(0.6, 0.999, 1.0) * t.base.scale.x;
    }
}

// ---------------------------------------------------------------------------------------------
// Floorplan.h
// ---------------------------------------------------------------------------------------------

pub fn floorplan(_gl: &Rc<glow::Context>, res: &Resources) -> Object {
    let mut o = Object::new();
    o.mesh = Some(res.acquire_mesh("floorplan.obj"));
    o.shader = Some(res.acquire_shader("texture_array"));
    o.texture = Some(res.acquire_texture("floorplan_textures.bmp", 4, 4));
    o.scale = Vector3::splat(0.1524); //6-inches to meters
    o
}

pub fn floorplan_add_portals(
    fp: &Object,
    gl: &Rc<glow::Context>,
    res: &Resources,
    pvec: &mut PPortalVec,
) {
    let p1 = Rc::new(RefCell::new(Portal::new(gl, res)));
    let p2 = Rc::new(RefCell::new(Portal::new(gl, res)));
    let p3 = Rc::new(RefCell::new(Portal::new(gl, res)));
    let p4 = Rc::new(RefCell::new(Portal::new(gl, res)));
    let p5 = Rc::new(RefCell::new(Portal::new(gl, res)));
    let p6 = Rc::new(RefCell::new(Portal::new(gl, res)));

    {
        let mut p = p1.borrow_mut();
        p.base.pos = Vector3::new(33.0, 10.0, 25.5) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
    }
    {
        let mut p = p2.borrow_mut();
        p.base.pos = Vector3::new(74.0, 10.0, 25.5) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
    }
    {
        let mut p = p3.borrow_mut();
        p.base.pos = Vector3::new(33.0, 10.0, 66.5) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
    }
    {
        let mut p = p4.borrow_mut();
        p.base.pos = Vector3::new(63.5, 10.0, 48.0) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
        p.base.euler.y = GH_PI / 2.0;
    }
    {
        let mut p = p5.borrow_mut();
        p.base.pos = Vector3::new(63.5, 10.0, 7.0) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
        p.base.euler.y = GH_PI / 2.0;
    }
    {
        let mut p = p6.borrow_mut();
        p.base.pos = Vector3::new(22.5, 10.0, 48.0) * fp.scale;
        p.base.scale = Vector3::new(4.0, 10.0, 1.0) * fp.scale;
        p.base.euler.y = GH_PI / 2.0;
    }

    connect_warps(&p1, Side::Front, &p3, Side::Back);
    connect_warps(&p1, Side::Back, &p2, Side::Front);
    connect_warps(&p3, Side::Front, &p2, Side::Back);

    connect_warps(&p4, Side::Front, &p6, Side::Back);
    connect_warps(&p4, Side::Back, &p5, Side::Front);
    connect_warps(&p6, Side::Front, &p5, Side::Back);

    pvec.push(p1);
    pvec.push(p2);
    pvec.push(p3);
    pvec.push(p4);
    pvec.push(p5);
    pvec.push(p6);
}

// ---------------------------------------------------------------------------------------------
// Sky.h
// ---------------------------------------------------------------------------------------------

pub struct Sky {
    mesh: Rc<Mesh>,
    shader: Rc<Shader>,
}

impl Sky {
    pub fn new(_gl: &Rc<glow::Context>, res: &Resources) -> Sky {
        Sky {
            mesh: res.acquire_mesh("quad.obj"),
            shader: res.acquire_shader("sky"),
        }
    }

    // PORT: takes the GL context explicitly for the two glDepthMask calls; C++ uses the
    // implicit current context (was: void Draw(const Camera& cam), Sky.h:12).
    // EXT: drawn after the scene rather than before it (Engine::render), at the far plane
    // under GL_LEQUAL so it passes exactly where the cleared depth survived -- the pixels
    // nothing else covered, including the holes a `discard` left. The original's sky-first
    // order shaded every pixel of every pass and then overdrew about half of them; the
    // sky's per-pixel atan/asin and two panorama taps were the most expensive thing being
    // thrown away. The depth mask stays off, as before.
    pub fn draw(&self, gl: &glow::Context, cam: &Camera) {
        unsafe {
            gl.depth_mask(false);
            gl.depth_func(glow::LEQUAL);
        }
        let mvp = cam.projection.inverse();
        let mv = cam.world_view.inverse();
        self.shader.use_program();
        self.shader.set_mvp(Some(&mvp), Some(&mv));
        // EXT: the sky shader samples the baked cloud panorama (src/ext/skybake.rs) and
        // scrolls it with the frame clock. The original sky had no texture at all.
        if let Some(sky) = crate::ext::skybake::inputs() {
            unsafe {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(sky.b));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(sky.a));
            }
            self.shader.set_i32("tex2", 1);
            self.shader.set_f32("blend", sky.blend);
        }
        self.shader.set_f32("time", crate::ext::view::time());
        // EXT: the eye is the translation of the inverse already computed for `mv` above.
        let eye = mv.translation();
        self.shader.set_f32("mood", crate::ext::view::mood_for(eye));
        self.mesh.draw();
        unsafe {
            gl.depth_func(glow::LESS);
            gl.depth_mask(true);
        }
    }
}

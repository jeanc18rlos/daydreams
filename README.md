# DayDreams

**DayDreams** is the game. Underneath it is a faithful Rust port of
**[HackerPoet/NonEuclidean](https://github.com/HackerPoet/NonEuclidean)**, CodeParade's
non-Euclidean rendering engine (MIT, © 2018 CodeParade). All credit for the engine, the level design,
the meshes, the textures and the shaders belongs to the original author — this repository
translates the C++/Win32/OpenGL source into Rust on top of winit + glutin + glow, and builds a game
on top of that.

The original's `LICENSE` is preserved verbatim as [`LICENSE-ORIGINAL-MIT`](LICENSE-ORIGINAL-MIT).
The video that introduced the project: [*Non-Euclidean Worlds Engine*](https://www.youtube.com/watch?v=kEB11PQ9Eo8).

The engine renders seven small scenes in which space does not behave: rooms larger on the inside,
tunnels that loop back on themselves, corridors that change your scale as you walk them, and an
infinite floorplan. It works by drawing portals into framebuffers recursively and warping the player
through the portal plane when they cross it — there is no ray marching and no distortion of the
geometry itself.

## What "port" means here

This is a transcription, not a rewrite. The C++ structure, order of operations, naming (converted to
`snake_case`) and quirks are preserved wherever Rust allows. `Matrix4` is row-major with translation
at `m[3]`, `m[7]`, `m[11]`, exactly as in `Vector.h`; multiplication orders that look backwards are
the original's and are left alone. No linear-algebra crate is used.

Every place where the port had to deviate carries a comment in a fixed form:

```rust
// PORT: <what changed> (was: <original C++>, <File.cpp:line>)
```

There are **226** such comments. Every one of them is summarised under
[Deviations from the original](#deviations-from-the-original) below. To read them in place:

```sh
grep -rn '// PORT:' src
```

## Building and running

Requires a Rust toolchain — stable, **1.85 or newer** (`rust-version` in `Cargo.toml`;
`rust-toolchain.toml` selects stable with `rustfmt` and `clippy`) — and a GPU/driver offering
**OpenGL 3.3 Core** or better. macOS grants 4.1 Core, which is sufficient. Only core-profile entry
points are used — no `EXT`/`ARB` suffixed calls, no immediate mode.

On Linux the native libraries gilrs, cpal and winit link against must be installed first; on
Debian/Ubuntu that is

```sh
sudo apt-get install libudev-dev libasound2-dev libdbus-1-dev libwayland-dev libxkbcommon-dev \
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libgl1-mesa-dev
```

(the same list `.github/workflows/ci.yml` installs). macOS and Windows need nothing beyond Rust.

```sh
cargo run --release                 # fullscreen
cargo run --release -- --windowed   # a 1280x720 window; what to use while developing
```

> **Run it from anywhere.** The original loads everything through *relative* paths (`Meshes/`,
> `Shaders/`, `Textures/`), so it had to be started from its own folder. The port resolves an
> asset root once at startup instead -- see [Asset root](#asset-root) below -- so `cargo run`,
> the bare binary and a `.app` bundle all find their files, and a launch that cannot is a
> dialog rather than a panic on the first missing shader.

A debug build is playable but not free: the fixed-step loop runs at 500 Hz (`GH_DT = 0.002`) and the
collision pass is `O(objects² × hitSpheres × colliders)`, so `Cargo.toml` sets `opt-level = 2` on the
dev profile to keep it real-time, and builds dependencies at `opt-level = 3` so they are compiled
once and cached. Release is still recommended.

```sh
cargo test --release   # 306 tests: Matrix4/Vector3 algebra, the .obj parser against the shipped meshes, the portal warps and the teleport, the collision push, the camera, the platform layer and the extensions' pure logic
```

The mesh tests read `Meshes/`, so a checkout without the assets fails them.

### Profiles

| Profile | What it is for | Output |
|---|---|---|
| `dev` | Editing. The crate at `opt-level = 2`, dependencies at 3. | `target/debug/daydreams` |
| `release` | Playing and profiling. `opt-level = 3`, fat LTO, one codegen unit, `panic = "unwind"` (the crash dialog needs the stack to unwind past it), `debug = 1` so a crash log carries line numbers. | `target/release/daydreams` (4.0 MB on macOS arm64, with the platform layer) |
| `dist` | Shipping. `release` with the symbol table stripped (`strip = "symbols"`, `debug = false`). | `target/dist/daydreams` (3.3 MB) |

```sh
cargo build --profile dist
```

### `just` targets

A [`Justfile`](Justfile) wraps the commands above so nobody has to remember the flags
(`cargo install just`; `just` alone lists them):

| Target | Does |
|---|---|
| `just run` / `just run-windowed` | `cargo run --release`, fullscreen or in a window. Extra arguments pass through. |
| `just test` | `cargo test --release`. |
| `just lint` | `cargo fmt --check` and `cargo clippy --release --all-targets -- -D warnings` — what CI runs. |
| `just deny` | `cargo deny check`: licences, advisories, duplicate crates (`deny.toml`; needs `cargo install cargo-deny`). |
| `just shot SCENE OUT` | Screenshot scene `SCENE` to `OUT` after 120 frames, windowed. See [Dev tooling](#dev-tooling). |
| `just bench [SCENE]` | 600 frames of a scene with vsync off, fullscreen; prints the frame-cost line. |
| `just gen-terrain` | Regenerate `Meshes/meadow_tile.obj` from `ext::terrain::height`. |
| `just dist` | `cargo build --profile dist`. |
| `just bundle` | `cargo bundle --profile dist`: `DayDreams.app` under `target/dist/bundle/osx/` (needs `cargo install cargo-bundle`). |

### Where the game writes

Settings go to the per-user config directory, `settings.toml` — the exact path per OS is under
[Settings](#settings--extsettingsrs) — and nowhere else. The log file location is printed at
startup. Nothing is written next to the binary or into the working directory, so the game runs
from a read-only `.app` and from a Finder launch whose working directory is `/`.

### Lints and formatting

Clippy's defaults are the rule and nothing is allowed crate-wide: `Cargo.toml`'s
`[lints.clippy]` table is empty, and the one `#[allow(clippy::..)]` in the source is
`too_many_arguments` on `Mesh::add_face`, the transcription of `Mesh.cpp:171`'s ten-argument
signature. Where a lint asked for an idiomatic rewrite of a transcribed line the
rewrite was mechanical and is noted in place — `GH_PI` is `f32::consts::PI` (the same bits as
`GameHeader.h`'s literal, which a test pins), `Mesh.cpp`'s line parser tests its prefixes with
`strip_prefix`, `Player.cpp:87`'s two-branch pitch limit is one `clamp`, and the level tests'
relations between tuned constants are `const { assert!() }` blocks, checked at compile time.
`rustfmt.toml` sets the 100-column, small-heuristics-off style the source was written in, and
the source is `cargo fmt` clean (two hand-grouped tables and the `Vector.h` matrix literals are
`#[rustfmt::skip]`); `clippy.toml` lifts `too_many_arguments` to nine for `ui.rs`'s `draw_quad`.

## Shipping

What is needed to hand the game to someone who does not have the repository.

### Layout

The asset-root resolution ([Asset root](#asset-root)) looks next to the executable before it
looks anywhere else, so a distributable is the `dist` binary with the four asset directories
beside it, plus the licences:

```
DayDreams/
  daydreams            (daydreams.exe on Windows)
  Meshes/  Textures/  Shaders/  assets/
  LICENSE              (the project's own MIT)
  LICENSE-ORIGINAL-MIT (CodeParade's, for the engine)
  THIRD_PARTY.md       (every shipped asset, its source and licence)
```

That is the zip layout for Windows and Linux, and what the CI `dist` job uploads. `tools/`,
`examples/` and the Cargo files are not part of it.

### macOS `.app`

[`cargo-bundle`](https://github.com/burtonageo/cargo-bundle) reads `[package.metadata.bundle]`
in `Cargo.toml` and builds the bundle:

```sh
cargo install cargo-bundle
just bundle            # = cargo bundle --profile dist
open target/dist/bundle/osx/DayDreams.app
```

The four resource directories and the three licence files land in `Contents/Resources/` and
the binary in `Contents/MacOS/`; the asset-root resolution looks in `Contents/Resources/` when
the binary is inside a bundle, so nothing needs a working directory. The bundle identifier
`com.daydreams.game` is a placeholder, and `icon = []` because no icon exists yet — add an
`.icns` and list it there when one does.

A `.app` that is not signed and notarized is quarantined by Gatekeeper on every machine but the
one that built it. The outline, each step needing an Apple Developer account:

1. `codesign --force --deep --options runtime --sign "Developer ID Application: <name> (<team>)" DayDreams.app`
2. `ditto -c -k --keepParent DayDreams.app DayDreams.zip`
3. `xcrun notarytool submit DayDreams.zip --keychain-profile <profile> --wait`
4. `xcrun stapler staple DayDreams.app`, then zip it again for distribution.

Crash logs from a `dist` build are **not symbolicated**: the profile strips the symbol table
and keeps no line tables (`strip = "symbols"`, `debug = false`), so a backtrace from a shipped
binary is addresses and whatever the exported names say. The `release` profile keeps line
tables (`debug = 1`), which on macOS live in the object files under `target/`, not in the
binary -- `dsymutil target/release/daydreams` collects them into a `.dSYM`. If shipped crash
logs are to name source lines, keep `debug = 1` in `dist` (and drop the strip), build the
`.dSYM` for each tagged build and keep it with the tag; as configured, they do not.

### Git LFS

The files the LFS patterns below match come to about 90 MB across 27 files (the 47 MB Escher
mesh, the 20 MB Backrooms GLB, the 20 MB soundtrack, the door GLB, the other meshes, the
font; `git ls-files -z | xargs -0 du -ch` filtered by the patterns), and the history holds
more: the door GLB was committed at 79 MB before its textures were shrunk. `.gitattributes` already routes `*.glb`,
`*.mp3`, `*.ttf` and `Meshes/*.obj` through Git LFS for files added from now on, but the files
already in history are ordinary blobs until they are rewritten. There is no remote yet, so the
rewrite is cheap; run it once, before the first push:

```sh
git lfs install
git lfs migrate import --everything --include="*.glb,*.mp3,*.ttf,Meshes/*.obj"
git lfs ls-files      # should list every one of them
```

`--everything` rewrites all branches, so do it when none are mid-merge. After it, clones need
`git lfs` installed, and CI checks out with `lfs: true`.

### CI

`.github/workflows/ci.yml` runs on every push to `main` and every pull request:

- **check**, on `macos-14`, `ubuntu-latest` and `windows-latest`: `cargo fmt --check`,
  `cargo clippy --release --all-targets -- -D warnings`, `cargo test --release`.
- **deny**: `cargo deny check` against `deny.toml`.
- **dist**, on tags `v*` only, after the other two: `cargo build --profile dist` on each OS and
  an artifact per platform in the layout above.

All three are green: `fmt --check` reports nothing, clippy is clean under `-D warnings` with
no crate-wide allows, and the tests pass (see [Status](#status)).

## Controls

| Input | Action |
| --- | --- |
| Mouse | Look |
| `W` `A` `S` `D` | Walk |
| `Shift` (hold) | Run — see [Running](#running--extsprintrs) |
| `1` – `7` | Load scene 1–7 |
| `Alt` + `Enter` | Toggle fullscreen |
| `Esc` | Quit |

The seven scenes, in key order and in the registration order of `Engine.cpp:41-47`:

| Key | Scene | Rust type |
| --- | --- | --- |
| `1` | Looping tunnels | `Level1` |
| `2` | House, 3 rooms | `Level2::new(3)` |
| `3` | House, 6 rooms | `Level2::new(6)` |
| `4` | Pillar room and statue | `Level3` |
| `5` | Sloped tunnels | `Level4` |
| `6` | Scaling tunnels | `Level5` |
| `7` | Infinite floorplan | `Level6` |

Scenes 2 and 3 are the same `Level2` type constructed with a different room count, which is why
seven scenes come from six level modules.

### A note on mouse feel on macOS

On Windows the original registers a raw-input device (`RegisterRawInputDevices`) and reads
`WM_INPUT` deltas, which bypass the OS pointer-acceleration curve entirely. On macOS, winit's
`DeviceEvent::MouseMotion` is **not** truly raw: the deltas have already been through the system's
pointer acceleration. The result is that look sensitivity is non-linear with speed — flicks travel
further than the same distance moved slowly — where the Windows original is perfectly linear.
`GH_MOUSE_SENSITIVITY` is unchanged from the original, so the feel differs slightly by design rather
than by tuning. There is no way to opt out short of an `IOHIDManager` device grab, which would mean
a new dependency.

## File mapping

| C++ | Rust | Notes |
| --- | --- | --- |
| `Vector.h` | `src/vector.rs` | `Vector3`, `Vector4`, row-major `Matrix4` |
| `GameHeader.h` | `src/game_header.rs` | the 25 `GH_*` constants, `gh_clamp`/`gh_min`/`gh_max` |
| `Sphere.h` | `src/sphere.rs` | |
| `Timer.h` | `src/timer.rs` | `QueryPerformanceCounter` → `std::time::Instant` |
| `Input.h/.cpp` | `src/input.rs` | plus `key_index`, the winit `KeyCode` → ASCII-slot map |
| `Camera.h/.cpp` | `src/camera.rs` | |
| `Collider.h/.cpp` | `src/collider.rs` | |
| `Mesh.h/.cpp` | `src/mesh.rs` | `.obj` parser factored into a testable `parse_obj` |
| `Shader.h/.cpp` | `src/shader.rs` | |
| `Texture.h/.cpp` | `src/texture.rs` | |
| `FrameBuffer.h/.cpp` | `src/frame_buffer.rs` | |
| `Resources.h/.cpp` | `src/resources.rs` | `weak_ptr` caches → `RefCell<HashMap<String, Weak<T>>>` |
| `Object.h/.cpp` | `src/object.rs` | `Object` + the `ObjectT` trait (the C++ vtable), `RenderCtx`/`UpdateCtx` |
| `Physical.h/.cpp` | `src/physical.rs` | |
| `Player.h/.cpp` | `src/player.rs` | |
| `Portal.h/.cpp` | `src/portal.rs` | `Warp`, `Side`, `connect`, `connect_warps` |
| `Scene.h` | `src/scene.rs` | `Scene` trait, `PObjectVec`, `PPortalVec` |
| `Ground.h` `House.h` `Pillar.h` `PillarRoom.h` `Statue.h` `Sky.h` `Tunnel.h` `Floorplan.h` | `src/props.rs` | the header-only props, collapsed into one module |
| `Level1.h/.cpp` | `src/level1.rs` | |
| `Level2.h/.cpp` | `src/level2.rs` | serves both scene 2 and scene 3 |
| `Level3.h/.cpp` | `src/level3.rs` | |
| `Level4.h/.cpp` | `src/level4.rs` | |
| `Level5.h/.cpp` | `src/level5.rs` | |
| `Level6.h/.cpp` | `src/level6.rs` | |
| `Engine.h/.cpp` | `src/engine.rs` | engine core only — see below |
| `Main.cpp` + the Win32 half of `Engine.cpp` | `src/main.rs` | window, GL context, event loop, fullscreen, cursor |

`Engine.cpp` is the one file that splits. Its engine logic (`Render`, `Update`, `LoadScene`,
`NearestPortalDist`) is in `src/engine.rs`; its platform layer — `CreateGLWindow`, `WindowProc`,
`SetupInputs`, `ConfineCursor`, `ToggleFullscreen` and the `PeekMessage` pump — moves to
`src/main.rs`, rebuilt on winit and glutin. That platform layer is the only genuine rewrite in the
port; everything else is transcription.

The four C++ globals are not ported as statics: `GH_ENGINE` and `GH_INPUT` are threaded through as
`&RenderCtx` / `&UpdateCtx`, and `GH_REC_LEVEL` / `GH_FRAME` become `Cell` fields on `Engine`.

## Assets

`Meshes/` (14 files), `Textures/` (7 files) and `Shaders/` (10 files) are the original's, byte-for-byte,
with one exception noted below.

`assets/` holds what the game adds on top:

| Path | What |
|------|------|
| `assets/fonts/RobotoCondensed[wght].ttf` | The UI face, Roboto Condensed (SIL OFL — the licence sits beside it). Vendored rather than taken from the system, because `tools/gen_ui.py` bakes it into `Textures/ui_font.bmp` and that atlas has to be reproducible. |
| `assets/music/` | Soundtrack. Streamed, not decoded up front — see Audio below. |
| `assets/sfx/` | One-shot effects, by name. |
| `assets/ui/` | Source art for the cursor atlas. |

---

# Deviations from the original

Everything below corresponds to a `// PORT:` comment in the source. Grouped by cause.

## 1. Approved, deliberate behaviour changes

These change what the program *does*, not just how it is written.

### Shaders rewritten for the core profile — `Shaders/*`

The only modified assets. GLSL removed `gl_FragColor` and `texture2D` when the core profile landed;
each fragment shader declares `out vec4 fragColor` instead, and `texture2D(` becomes `texture(`.
Nothing else in the shaders changed.

### Mesh draw-call bug fixed — `mesh.rs:490`

`Mesh::Draw` passes `(GLsizei)verts.size()` to `glDrawArrays` — the number of **floats**, i.e. three
times the real vertex count. The original reads two vertex-strides past the end of every buffer on
every draw. Ported as `verts.len() / 3`.

### GL objects are freed — the `Drop` impls in `mesh.rs`, `shader.rs`, `texture.rs` and `frame_buffer.rs`

`Mesh`, `Shader` and `Texture` never delete their GL objects in C++, so every scene switch leaked
whatever the old scene had uploaded; each gets a `Drop` impl that deletes what it owns, and the
resource caches' `Weak` handles let that run when the last user goes. `FrameBuffer` had no
destructor either, and the original leaked roughly 78 of them — about 1.8 GiB of texture and
renderbuffer memory — across the seven scenes, because each portal owned three. Its `Drop` now runs
on a window resize and at shutdown rather than per scene: the engine owns the only three, shared by
every portal (see [Load time and frame cost](#load-time-and-frame-cost)).

### Occlusion queries no longer leak — `engine.rs:419`, `engine.rs:438`

`glGenQueriesARB` sits *outside* the `GH_REC_LEVEL > 0` block that deletes the queries
(`Engine.cpp:220-222`), so at the deepest recursion level one GL query per portal is created and
never deleted, every frame. Creation moves into the same block as the deletion; the queries are used
nowhere else.

### Mipmap generation for the texture array — `texture.rs:92`

`GL_GENERATE_MIPMAP` is a compatibility-profile texture parameter that does not exist in core (and
glow does not even expose the enum). The original's own `glGenerateMipmap` call names
`GL_TEXTURE_2D`, the wrong target for an array texture, so it is a no-op. Both are dropped and
replaced by an explicit `generate_mipmap(TEXTURE_2D_ARRAY)` placed *after* `tex_image_3d`. This
reproduces the Windows compat-profile behaviour of auto-generating mipmaps on upload — required,
because the min filter is `LINEAR_MIPMAP_NEAREST` and a mipmap-less array texture samples black.
(The original's inverted `width/rows` ÷ `height/cols` operands are transcribed literally; harmless,
because the only atlas is a square 4×4.)

### Silent failures become loud ones

The original swallows every asset and GL error, then renders nothing and gives you no clue why.

| Site | C++ behaviour | Port |
| --- | --- | --- |
| `shader.rs` | link failure → writes `<vert>.link.log`, sets `progId = 0`, returns | `AssetError::ShaderLink` carrying the info log |
| `shader.rs` | missing shader file → empty source string, then a compile error | `AssetError::Io` |
| `shader.rs` | compile failure → writes `<fname>.log`, returns 0 | `AssetError::ShaderCompile` carrying the info log |
| `shader.rs` | no `;` after `"\nin "` → walks backwards from `npos` (UB) | `AssetError::ShaderCompile` |
| `texture.rs` | missing `.bmp` → `texId = 0`, binds the default texture forever | `AssetError::Io`; a truncated or non-24/32-bit file is `AssetError::BadBmp` |
| `mesh.rs` | failed `.obj` open → returns from the ctor before `glGen*`, leaving garbage handles that `Draw` then uses | `AssetError::Io` -- a mesh that is asked for and not shipped is a packaging bug, not an invisible object |
| `frame_buffer.rs` | incomplete FBO → returns silently, half-constructed | same control flow, plus a warning; the `glGen*` failures are `AssetError::Gl` |

The loaders return the error; `Resources::acquire_*` (and the glTF `acquire`) stay infallible
for the scene code and route any `Err` through one sink, `app::crash::fatal`, which logs it,
shows the [crash dialog](#crash-dialog) and exits with code 1. See [Typed asset
errors](#typed-asset-errors-and-the-fatal-sink).

### Immediate-mode debug drawing dropped — `collider.rs:70`, `mesh.rs:509`, `object.rs:109`, `engine.rs:499`

`Collider::DebugDraw` is written in `glBegin`/`glColor3f`/`glVertex4f`/`glEnd`, which does not exist
in a 3.3+ core profile and which glow exposes no entry points for. It is dropped, and with it
`Mesh::DebugDraw` and `Object::DebugDraw`, which are thin wrappers over it, plus the `#if 0` debug
block at `Engine.cpp:264-269` that was their only caller.

### Cursor handling on macOS — `main.rs:142`

The original warps the pointer to the window centre every frame. Here that only happens on the
*confined* fallback path: on macOS, winit's `set_cursor_position` finishes by re-associating the
cursor with the pointer, which silently undoes a prior `Locked` grab — so warping on the locked path
would break mouse look after exactly one frame. Related: `main.rs:444` deliberately ignores
`WindowEvent::CursorMoved` (its absolute position stops changing once the cursor is locked) and
drives look from `DeviceEvent::MouseMotion` deltas instead; `main.rs:372` re-applies the grab on
focus gain, since alt-tabbing drops it on every platform.

### Dropped code that nothing calls

- `vector.rs:315` — the free `operator/=(float, Vector3&)` (`Vector.h:136-138`) assigns into its
  *second* operand, which Rust's `DivAssign` cannot express. Uncalled.
- `timer.rs:36` — `Start()` / `Stop()` / `StopStart()`. `Engine.cpp` only uses `GetTicks` and
  `SecondsToTicks`.
- `game_header.rs:13` — `GH_CLASS`, the Win32 window-class name passed to `RegisterClassEx`. winit
  owns the window class; nothing can consume it.

## 2. Core-profile and glow API translations

Mechanical, no behaviour change.

- **`GL_CLAMP` → `GL_CLAMP_TO_EDGE`** (`frame_buffer.rs:32`, `:39`). `GL_CLAMP` was removed in core.
- **Unsized `GL_RGB` → sized `GL_RGB8`** (`frame_buffer.rs:56`) for the FBO colour attachment; an
  unsized internal format is not a legal attachment format in core. The external format stays
  `GL_RGB`; `nullptr` pixel data becomes `PixelUnpackData::Slice(None)`.
- **All `*EXT` framebuffer/renderbuffer calls → core** (`frame_buffer.rs:73`, `:86`, `:99`, `:111`):
  `glGenFramebuffersEXT`, `glGenRenderbuffersEXT`, `GL_DEPTH_ATTACHMENT_EXT`,
  `glCheckFramebufferStatusEXT` and friends.
- **All `*ARB` query calls → core** (`engine.rs:452`, `:466`, `:478`): `glBeginQueryARB` →
  `begin_query`, `GL_SAMPLES_PASSED_ARB` → `glow::SAMPLES_PASSED`, `glGetQueryObjectuivARB` →
  `get_query_parameter_u32` (which returns the value rather than writing through a pointer),
  `glDeleteQueriesARB(n, ptr)` → one `delete_query` per handle.
- **Framebuffer name 0 → `None`** (`frame_buffer.rs:128`, `engine.rs:245`, `:389`). glow models the
  default framebuffer as `Option::None`; `GLuint curFBO` becomes `Option<glow::Framebuffer>`.
- **Occlusion-support query dropped** (`engine.rs:106`). glow exposes no `glGetQueryiv`, so
  `glGetQueryiv(GL_SAMPLES_PASSED_ARB, GL_QUERY_COUNTER_BITS_ARB, &occlusionCullingSupported)` cannot
  be transcribed. Occlusion queries on `GL_SAMPLES_PASSED` are core since GL 1.5 and mandatory in
  3.3+ core, so the answer is unconditionally "supported" on any context this port can run on.
- **`glewInit()` has no equivalent** (`engine.rs:93`, `main.rs:297`) — glow resolves its function
  pointers when the context is created.
- **Explicit context everywhere** (`frame_buffer.rs:18`, `resources.rs:21`, `engine.rs:45`,
  `camera.rs:93`, `props.rs:312`, `mesh.rs:528`, `object.rs:74`). C++ reaches the GL through GLEW's
  global function pointers; glow needs a context value, so every GL-owning type carries an
  `Rc<glow::Context>` and the free functions take one.
- **`glBufferData(void*, bytes)` → `buffer_data_u8_slice(&[u8])`** (`mesh.rs:528`).
- **`GLuint` handles → glow's opaque handle types** (`frame_buffer.rs:13`).

## 3. C++ language features Rust does not have

### Uninitialized members

The C++ default constructors `Vector3() {}`, `Vector4() {}` and `Matrix4() {}` leave their fields as
garbage. Rust has no such thing, so they are zero-initialized and then overwritten exactly as the
C++ does (`vector.rs:15`, `:323`, `:428`). Same story at `camera.rs:24` (`near`/`far`),
`collider.rs:17` (`Collider::mat`, which `CreateSorted` overwrites unconditionally anyway),
`portal.rs:48` (both warp matrices, immediately `MakeIdentity()`'d), `physical.rs:30` and
`player.rs:32` (fields zeroed before the `Reset()` the ctor relies on), and `engine.rs:414`
(`GLuint drawTest[GH_MAX_PORTALS]`, only ever read on the path that also writes it).

### Overloading and default arguments

Overload sets become distinctly named functions:

| C++ | Rust |
| --- | --- |
| `Vector3(float)` / `Vector3(const float*)` / `Vector3(float,float,float)` | `splat` / `from_slice` / `new` (`vector.rs:26`) |
| `Vector4(float)` / `Vector4(const Vector3&, float)` / `Vector4(f,f,f,f)` | `splat` / `from_vec3` / `new` (`vector.rs:337`) |
| `Matrix4::Scale(float)` / `Matrix4::Scale(const Vector3&)` | `scale_uniform` / `scale` (`vector.rs:544`) |
| `Vector3 Matrix4::Scale() const` | `get_scale` — renamed to avoid the static `scale()` (`vector.rs:576`) |
| `Sphere(float r = 1.0f)` / `Sphere(const Vector3&, float)` | `new` / `new_at` (`sphere.rs:15`) |
| `AquireTexture(const char*, int rows = 1, int cols = 1)` | every caller passes them explicitly (`resources.rs:74`) |

### Reserved words and name collisions

`Use()` → `use_program` / `use_texture` (`shader.rs:82`, `texture.rs:156`, `frame_buffer.rs:141`);
`Player::Move` → `move_player` (`player.rs:140`); `Player::Update` → `update_player`, so it does not
collide with `Physical::update` reachable through `base` (`player.rs:64`); `Tunnel::type` → `ttype`
(`props.rs:160`); `Object::Reset` → `reset_obj` on the trait, so it does not collide with the
inherent `reset`s (`object.rs:160`).

### Inheritance → composition

`Physical : Object` (`physical.rs:9`), `Player : Physical` (`player.rs:12`), `Portal : Object`
(`portal.rs:60`) and `Tunnel : Object` (`props.rs:158`) all become a `base` field plus an
`impl ObjectT`, which carries the virtual interface. `player.rs:212` adds `obj()` / `obj_mut()`
because reaching the `Object` through two levels of composition (`player.base.base`) is noisy.

`object.rs:40` writes `Default` for `Object` by hand rather than deriving it — the C++ ctor sets
`scale` and `p_scale` to 1, which `#[derive(Default)]` would get wrong.

### `assert` → `debug_assert!`

C's `assert` compiles out of release builds, so `debug_assert!` is the exact analogue:
`vector.rs:155`, `sphere.rs:31`, `collider.rs:78`, `texture.rs:16`, `:41`, `portal.rs:106`.
`engine.rs:292` drops `assert(vObjects[i].get())` entirely — an `Rc` is never null.

### `#if 0` blocks

Rust has no `#if 0`. The jumping block at `Player.cpp:61-66` is preserved verbatim as a never-called
private fn (`player.rs:107`). The debug-collider block at `Engine.cpp:264-269` is dropped with the
immediate-mode code it called (`engine.rs:499`).

### Miscellaneous type mappings

- `template<class T>` with C++'s implicit `operator<` requirement → an explicit `T: PartialOrd` bound
  (`game_header.rs:58`).
- `GH_MAX_PORTALS`: `const int` → `usize`, since it only ever sizes a `Vec` (`game_header.rs:20`).
- `GH_SCREEN_WIDTH`/`HEIGHT`: `const int` → `u32`, to match winit's `PhysicalSize<u32>`
  (`game_header.rs:28`, `:31`).
- `FLT_EPSILON` → `f32::EPSILON` (`vector.rs:133`, `:142`); `FLT_MAX` → `f32::MAX` (`engine.rs:517`).
- `memset` → array `fill` (`input.rs:44`, `:46`).
- `delete[] img` → implicit, `img` is a `Vec` (`texture.rs:145`).
- `*reinterpret_cast<int32_t*>(&input[18])` → `from_le_bytes`; BMP headers are always little-endian,
  matching the original's x86 target (`texture.rs:34`).
- `std::shared_ptr<T>` → `Option<Rc<T>>`, a null `shared_ptr` being `None` (`object.rs:33`;
  `Portal::errShader` takes the same mapping, `portal.rs:67`).
- `GH_PI`: the literal `3.141592653589793f` → `f32::consts::PI`, the same 32 bits; a test in
  `game_header.rs` pins them.
- `Player.cpp:87`'s two-branch pitch limit → one `clamp`, identical for every value, NaN included
  (`player.rs:164`).
- `WM_SIZE`'s zero-dimension guard is the `Resized` arm's match guard rather than an inner `if`; a
  zero-size resize falls through to the empty arm as before (`main.rs:425`).
- Fixed-size C array → `Vec`, length expression unchanged (`portal.rs:69`).
- `for (int i = 0; cond; ++i)` → a manual counter; Rust has no C-style `for` (`engine.rs:213`).
- `int64_t GetTicks()` is non-`const` in C++ only because it stores into a scratch field; it takes
  `&self` here, and the scratch fields are gone (`timer.rs:11`, `:26`).

## 4. Borrow-checker accommodations

Places where C++'s freedom with aliasing had to be expressed differently. None change behaviour.

### Out-parameters become `Option`

`bool Collide(const Matrix4&, Vector3& delta)` → `collide(..) -> Option<Vector3>` (`collider.rs:42`,
`engine.rs:341`). `const Warp*` → `Option<&Warp>` (`physical.rs:97`, `portal.rs:183`).
`void SetMVP(const float*, const float*)` took two nullable raw pointers → `Option<&Matrix4>`
(`shader.rs:158`).

### Portal identity: raw `this` pointers → `u32` ids

`Warp` stores `const Portal* fromPortal / toPortal`, used **only** for identity comparison against
`skipPortal` (`Engine.cpp:237/244/253`). Rust cannot hand out a raw self pointer from a constructor
and later compare it through an `Rc`, so each portal takes a unique `u32` from a counter and `Warp`
stores ids (`portal.rs:15`, `:41`, `:64`).

`Portal::Connect(Warp& a, Warp& b)` is called as `Connect(p1->front, p3->back)` — two mutable
references into two other objects at once, which borrowck forbids through `RefCell`. It becomes the
free function `connect_warps(a, a_side, b, b_side)`, with a new `Side` enum naming the warp
(`portal.rs:23`, `:223`, `:253`). `Portal::Connect(shared_ptr&, shared_ptr&)` likewise becomes the
free fn `connect` (`portal.rs:245`). `portal.rs:262` reads both transforms and drops the borrows
before writing anything, because `a` and `b` may be the same cell.

### Dropped `Object& other` parameters

`Object::OnHit(Object& other, Vector3& push)` and `Physical`/`Player::OnCollide(Object& other, const
Vector3& push)` lose the `other` argument — no implementation in the codebase reads it, and passing
it would require borrowing two `RefCell<dyn ObjectT>` cells mutably at once (`object.rs:117`,
`physical.rs:71`, `player.rs:164`, `engine.rs:346`).

### `on_collide` on the trait — `object.rs:139`

Added, with no C++ counterpart. `Physical::OnCollide` is virtual and `Player` overrides it, but the
contract's `as_physical_mut()` hands out a `&mut Physical`, on which Rust would *statically* resolve
`on_collide` to the base implementation and silently lose `Player`'s `onGround` handling. `Engine`
therefore calls `obj.on_collide(push)` through the trait, which dispatches virtually. The default
impl reproduces the C++ behaviour for every non-`Player` `Physical`.

### Collision loop restructured — `engine.rs:303`, `:323`

`Physical* physical = vObjects[i]->AsPhysical()` cannot be held across the loop body: it would keep
`vObjects[i]` borrowed while `vObjects[j]` is borrowed mutably and while `on_collide` re-borrows it.
The two things the C++ reads through that pointer (the hit spheres and `worldToLocal`) are pulled out
first, and every write goes back through a fresh borrow. Likewise the `Rc<Mesh>` is cloned out of
`vObjects[j]` so the cell can be re-borrowed.

### `Camera` copied out of its `RefCell` — `engine.rs:241`

`Camera` is `Copy`. `Render` recurses arbitrarily deep and must not hold a borrow of `main_cam`, so
the camera is copied before rendering.

### `RefCell` and `Rc<RefCell<..>>` on the shared state

`PObjectVec` becomes `Vec<Rc<RefCell<dyn ObjectT>>>` — `dyn ObjectT` is the equivalent of the C++
vector of base-class pointers dispatching through `Object`'s vtable, and the `RefCell` restores the
interior mutability a raw C++ pointer has for free (`scene.rs:17`). `PPortalVec` holds a concrete
`Portal`, not `dyn`, because `Engine` calls `Portal`-specific methods on it (`scene.rs:22`).
`Engine::main_cam` and `Engine::input` become `RefCell` because `Run` mutates them through an
otherwise-shared `&self` (`engine.rs:51`, `:54`). `level2.rs:38` keeps `house2` as an `Option`,
mirroring the default-constructed-null `shared_ptr` that is only `reset` when `num_rooms > 4`.

> **`draw` takes `&self`, never `&mut self`.** `Portal::draw` recurses back into `Engine::render`,
> which can reach the *same* portal again — `skipPortal` is `warp->toPortal`, the portal on the far
> side, not the one being drawn. A nested `borrow_mut()` would panic; a nested `borrow()` is fine, so
> the whole render path stays immutable.

## 5. Globals threaded through as parameters

`GH_ENGINE` and `GH_INPUT` (`GameHeader.h:48-50`) are not ported as statics. They become the
`RenderCtx` and `UpdateCtx` structs (`object.rs:11`), passed explicitly:
`GH_ENGINE->NearestPortalDist()` → `ctx.engine.nearest_portal_dist()` (`portal.rs:133`),
`GH_INPUT` → `ctx.input` (`player.rs:64`), `FrameBuffer::Render` takes a `ctx` (`frame_buffer.rs:149`).
`GH_REC_LEVEL` and `GH_FRAME` become `Cell` fields on `Engine` (`engine.rs:73`, `:75`), read through
`RenderCtx` (`portal.rs:112`, `engine.rs:164`). `GH_FRAME` is never read by anything — it is kept
because the original keeps it.

The resource caches are function-local statics in `Resources.cpp`; they become an `Engine` member so
scenes can reach them without a global (`engine.rs:47`). Consequently `Scene::Load` gains `gl` and
`res` parameters (`scene.rs:27`), and so do the prop constructors and `Sky::draw`
(`props.rs:312`); `floorplan_add_portals` and `Portal::new` take `res` alone, since the portal
framebuffers `Portal::new` once built from `gl` live on the engine now.

`Resources` itself: `map[std::string(name)]` default-inserts an empty `weak_ptr` and returns a
reference to it, which is exactly `entry(..).or_insert_with(Weak::new)` (`resources.rs:43`), and
`expired()` + `lock()` collapse into a single `upgrade()` (`resources.rs:47`).

## 6. Structural liberties

Only two were taken. Both are called out in the source.

### `parse_obj` split out of `Mesh::new` — `mesh.rs:11`

`Mesh::Mesh` parses the `.obj` straight into its own members and then uploads them
(`Mesh.cpp:8-153`). Here the parse is factored into a pure `parse_obj` that touches no OpenGL, so the
reader can be unit-tested without a GL context — which is what `mesh::tests::all_meshes_parse` does
against all 14 shipped meshes. `Mesh::new` calls it and then performs exactly the upload block of
`Mesh.cpp:130-152`. `ParsedMesh` holds what were the C++ members plus `is3DTex`, a ctor local
(`Mesh.cpp:18`) that the upload block read directly. `mesh.rs:420`: the C++ keeps `verts`/`uvs`/
`normals` alive for the Mesh's whole lifetime purely so `Draw` can read `verts.size()`; only the
count survives here.

### The header-only props collapsed into `props.rs`

`Ground.h`, `House.h`, `Pillar.h`, `PillarRoom.h`, `Statue.h`, `Sky.h`, `Tunnel.h` and `Floorplan.h`
each declare one trivial `Object` subclass with a constructor and a couple of setters. They become
free functions in one module. `props.rs:61`: the C++ `SetDoorN` helpers take `Object& portal` (the
base class) even though every call site passes a `Portal`; the contract types them as `&mut Portal`,
so the fields are reached through `portal.base`. `props.rs:148`: `enum Type { NORMAL = 0, SCALE = 1,
SLOPE = 2 }` becomes a Rust enum — the explicit discriminants are never used numerically.

Related smaller reshuffles: `Shader`'s `std::vector<std::string> attribs` member (`Shader.h:17`) is
only written by `LoadShader` and read by the ctor, so it becomes a local threaded through
(`shader.rs:10`, `:32`, `:90`). `Texture` slurps the whole file into a `Vec<u8>` and walks it with a
cursor instead of streaming through an `ifstream` (`texture.rs:22`). `Engine`'s `sky` is a plain
field rather than a `shared_ptr` — C++ needs the indirection only because `Sky` is constructed after
the GL context exists (`engine.rs:60`) — and it is therefore built *before* `LoadScene(0)` rather
than after (`engine.rs:118`); the only observable difference is the insertion order of
`quad.obj`/`sky` in the resource caches, which nothing depends on. `Run`'s two loop locals
(`Engine.cpp:71-72`) become fields, since the fixed-step loop is now driven one frame at a time from
winit's `about_to_wait` and cannot keep them on the stack (`engine.rs:79`); `ticks_per_step` is
constant and computed once (`engine.rs:137`).

## 7. The `.obj` parser's C++ stream semantics

`Mesh.cpp` leans on `std::istringstream` behaviour that Rust has no equivalent for, so it is
reproduced explicitly (`mesh.rs:39`): a failed extraction sets the fail flag and zeroes the target
(C++11 `num_get`), which is what `ss.fail()` is tested for at `Mesh.cpp:37` and `:94`; and once the
fail flag is set, every later extraction is a no-op. (C++ leaves the target *unmodified* on an
already-failed stream rather than zeroing it — every call site gates on the fail flag before reading
those values, so the difference is unobservable.)

`mesh.rs:47`: C++ `>>` consumes the longest valid numeric *prefix* of a token; this reads the whole
whitespace-delimited token. All 14 shipped `.obj` files use clean whitespace-separated numbers, so
the two agree. `mesh.rs:118`: `std::string::operator[]` returns `'\0'` at `index == size()`,
reproduced so `line[2]` / `line[3]` on a short line behave the same. `mesh.rs:206`:
`while (!fin.eof()) { std::getline(...) }` becomes a `.lines()` iteration — the C++ loop performs one
extra pass on an empty `line` when the file ends in a newline, which matches no prefix and is a
no-op, and `str::lines` strips a trailing `\r` the way Windows text-mode `getline` does.
`mesh.rs:254`: the C++ mutates `line` in place replacing `/` with a space; `line` borrows the input
here, so the edit is made on an owned copy.

Four sites use `wrapping_sub` to preserve C++'s unsigned wraparound exactly, rather than panicking on
an underflow the original quietly tolerated: `mesh.rs:147`, `:246`, `:263`, `:296`.

`shader.rs:21` is the same class of thing: `str.find(pat, from)` has no std equivalent, so the
offset search is written out.

## 8. The platform layer

`src/main.rs` is the one part of the port that is a rewrite rather than a transcription. It replaces
`WinMain` (`Main.cpp:4-16`, whose `_DEBUG` `AllocConsole`/`freopen` block has no equivalent — a Rust
binary already has stdout) and the entire Win32 half of `Engine.cpp`: `CreateGLWindow`
(`330-413`), `WindowProc` (`272-328`), `SetupInputs` (`440-467`), `ConfineCursor` (`469-475`),
`ToggleFullscreen` (`485-501`) and the `PeekMessage` pump inside `Run` (`77-127`)
(`main.rs:1`, `:5`, `:247`, `:341`).

- **Window creation** (`main.rs:65`) replaces the `WNDCLASSEX` registration and `CreateWindowEx`. The
  size is logical, matching `CreateWindowEx`'s DPI-aware pixel size after `SetProcessDPIAware`.
- **Pixel format** (`main.rs:83`) replaces `ChoosePixelFormat` + `PIXELFORMATDESCRIPTOR`. The
  original asks for 32-bit colour, 32-bit depth, double-buffered RGBA. 32-bit depth is not offered by
  every driver (macOS CGL tops out at 24 + 8 stencil), so the template asks for 24 and the picker
  takes the deepest offered.
- **Context creation** (`main.rs:100`) replaces `wglCreateContext`, which on Windows returns a legacy
  *compatibility* context. This asks for 3.3 Core explicitly — which is what forces several of the
  deviations in section 2.
- **`wglMakeCurrent`** → `main.rs:289`. **`wglSwapIntervalEXT(1)`** → glutin's
  `set_swap_interval(SwapInterval::Wait(1))`, which moves from `Engine.cpp:431` to `main.rs:307`
  (`engine.rs:113`) — the original's typo in the surrounding code is preserved above it.
- **`ShowCursor(FALSE)` + `ClipCursor`** → `main.rs:123`, which returns whether the pointer was
  *locked* rather than merely confined; see the macOS note in section 1.
- **Fullscreen** (`main.rs:220`): `SetWindowLong(GWL_STYLE, WS_POPUP)` + `SetWindowPos(HWND_TOPMOST,
  …)` becomes winit's borderless fullscreen; the windowed branch restores the original size and
  position. `iWidth`/`iHeight` are deliberately *not* written here — winit reports the new size
  through `WindowEvent::Resized`, which is also where the GL surface is resized.
- **`iWidth` / `iHeight`** (`main.rs:190`, `:320`) are **physical** pixels. glutin's macOS surface
  sets `setWantsBestResolutionOpenGLSurface(true)`, so the GL drawable is the physical size —
  typically 2× the logical window on a retina display. Feeding logical sizes to `glViewport` and
  `Camera::SetSize` would render into the bottom-left quarter of the window. (The C++ sets them to
  the logical constants at `Engine.cpp:352-353`.) `main.rs:352` guards against a zero dimension: the
  first frame and a minimised window can report 0, and `NonZeroU32::new` would panic.
- **`SetupInputs`** has no equivalent (`main.rs:328`): winit's `DeviceEvent::MouseMotion` already
  delivers what `RegisterRawInputDevices` asked for. Correspondingly `input.rs:54` and `:63` replace
  the mouse-move and button halves of `Input::UpdateRaw`, and `main.rs:427` feeds the buttons in.
- **Keyboard** (`input.rs:83`): the `WndProc`'s `input.key[wParam & 0xFF]` virtual-key indexing
  becomes `key_index`, mapping winit `KeyCode`s into the same ASCII slots the ported code reads
  (`'W'`, `'A'`, `'S'`, `'D'`, `'1'`..`'7'`, `' '`). `player.rs:112` notes that `VK_SPACE == 0x20 ==
  b' '`. `main.rs:391` preserves the key-repeat guard `if (lParam & 0x40000000) return 0;`.
  `engine.rs:170` adds the entry point main.rs uses to feed these in.
- **The frame loop**: `Run`'s prologue becomes `engine.rs:183`, and the body of its "no pending
  message" `else` branch becomes `engine.rs:190` / `main.rs:469`. `ControlFlow::Poll`, not `Wait`
  (`main.rs:502`) — the C++ renders whenever `PeekMessage` finds nothing to do. `ConfineCursor` and
  `SwapBuffers` stay in `main.rs` with the window.
- **Shutdown ordering** (`main.rs:171`): `Engine` owns every GL object and their `Drop` impls issue
  real GL calls, so it is declared first and therefore dropped first — while the context is still
  current. `engine.rs:504` is the `Unload` call, made from `main.rs` when the event loop exits;
  `engine.rs:506` makes the always-set-by-then `curScene` an explicit `Option`.
- **glutin's two-phase `DisplayBuilder`/`Init` dance** (`main.rs:156`): on most platforms `resumed`
  fires once, but Android tears the surface down and re-creates it, so the display and context are
  built only on the first pass.
- `WM_SYSCOMMAND` and `WM_PAINT` have no winit equivalent and are dropped (`main.rs:341`).
- `engine.rs:41` records where the rest of `Engine`'s Win32 members went: `hDC`/`hRC`/`hWnd`/
  `hInstance` and `iWidth`/`iHeight`/`isFullscreen` now live in `main.rs` (`main.rs:183`, `:185`,
  `:187`), and the window size arrives as parameters to `run_frame`.

## 9. Trivia

- `object.rs:22` — `typedef … PObjectVec` (`Object.h:46`) lives in `scene.rs` instead, next to
  `PPortalVec`, so the two aliases sit together.
- `object.rs:69` — `draw_impl` is the body of `Object::Draw` (`Object.cpp:20-31`), split out under a
  separate name so `ObjectT::draw` (the virtual) can call it as its default. The `curFBO` parameter is
  dropped from the base implementation, which never used it; only `Portal::Draw` does.
- `portal.rs:151` — `mesh` and `shader` are `Option<Rc<..>>` on the ported `Object` while C++
  dereferences the `shared_ptr`s unconditionally. `Portal::new` always fills both, so `unwrap`
  reproduces the C++ behaviour exactly.
- `portal.rs:162` — `DrawPink` takes a `ctx` only so it matches the call sites that already hold one.
- `collider.rs:44` — the parameter is named `local_to_unit` after the `.cpp` definition; the
  declaration at `Collider.h:9` confusingly calls the same parameter `localToWorld`.
- `collider.rs:6` — `Collider::mat` stays private exactly as in `Collider.h:16`; `Copy` is derived so
  colliders can be passed by value the way the C++ stores them in `std::vector<Collider>`.
- `engine.rs:66` — `GLint occlusionCullingSupported` → `bool`.
- `engine.rs:87` — `Engine::Engine()` minus the window/GL/raw-input setup, which `main.rs` does first.
- `engine.rs:124` — the seven scene registrations keep their order; that is what makes `1`–`7` select
  them.
- `engine.rs:266`, `:276` — `Scene::Load` gains `gl`/`res`; the `Rc<RefCell<Player>>` unsize-coerces
  to `Rc<RefCell<dyn ObjectT>>`, the equivalent of pushing a `shared_ptr<Player>` into a
  `vector<shared_ptr<Object>>`.
- `engine.rs:177` — a method nothing calls, kept for completeness.
- `level1.rs:14` — the C++ level classes have no state and no members; unit structs are the same
  thing.
- `level1.rs:27` — `std::shared_ptr<Tunnel> tunnel1(new Tunnel(...))` → an `Rc<RefCell<Tunnel>>` kept
  locally after being pushed into `objs`, because `SetDoor1`/`SetDoor2` are called on it later.
- `level1.rs:69` — `player.SetPosition(..)` is `Physical::SetPosition`, reached through `base`.
- `level2.rs:81`, `:97` — the `house2` `Option` is unwrapped in the branch where the C++ dereferences
  a `shared_ptr` that is only non-null there; `Portal::Connect(Warp&, Warp&)` becomes
  `connect_warps(portal, side, portal, side)`.
- `physical.rs:24`, `player.rs:23`, `props.rs:160` — snake_case renames (`hitSpheres`, `onGround`,
  `type`).
- `resources.rs:39`, `:59` — `const char*` → `&str`, `shared_ptr` → `Rc`.
- `game_header.rs:58` and the unused-but-preserved helpers (`gh_max`, `Matrix4::zero`,
  `Vector4::homogenized`, `Camera::inverse_projection`, `Player::cam_to_world`, …) carry
  `#[allow(dead_code)]` rather than being deleted. Fidelity to the original's API surface is the
  point; nothing ported was removed merely because the seven shipped scenes do not reach it.

---

## Status

`cargo build --release` — 0 errors, 0 warnings; `cargo build --profile dist` — clean.
`cargo test --release` — 306 passed, 0 failed.
`cargo clippy --release --all-targets -- -D warnings` — clean, with an empty `[lints.clippy]` table.
`cargo fmt --check` — clean.
`cargo deny check` — advisories, bans, licences, sources ok.

---

# Extensions beyond the port

Everything above documents the faithful port. Everything below is **new work** — it has no C++
counterpart and is not part of HackerPoet/NonEuclidean.

The split is enforced by convention and visible in the source: the port carries **226
`// PORT:` comments** citing the original line each deviation came from, while additions carry
**`// EXT:`** comments. New code lives in `src/ext/` and `src/level7..18.rs`; the ported files
were touched only where a hook was unavoidable, and each of those is a handful of lines.

## New scenes

| Key | Scene | Idea |
|----|-------|------|
| `8` | **Perspective Gallery** | Forced-perspective grabbing. Pick something up and where you release it decides how big it really is. |
| `9` | **Penrose Ascent** | Four descending corridors wired into a closed cycle. Walk forward to fall forever, turn around to climb forever. |
| `0` | **Compound** | Carry a grabbed object through a scaling portal so both size effects multiply. Neither source game does this. |
| `-` | **Unobserved** | Statues that only move while you are not looking at them, across two portal-linked chambers. |
| `=` | **Anamorphic Chamber** | Twelve scattered fragments that resolve into a ring from exactly one spot in the room. |
| `'` | **Backrooms** | The intro's meadow and door, but the door opens onto a scanned, light-baked office maze with real wall, floor and furniture collision -- and closes behind you. NEW GAME starts here. See [Backrooms](#backrooms-scene-) below. |
| `,` | **Pool Rooms** | A white-tiled hall of pillars, flooded knee-deep: the water is a real translucent surface the player wades through, the tiles under it the floor. See [Pool Rooms and Overgrown](#pool-rooms-and-overgrown-scenes--and-) below. |
| `.` | **Overgrown** | A backrooms maze gone to grass and bushes under a plaster ceiling, two rusted doors with lit EXIT signs. Same section. |

Scenes `1`–`7` are CodeParade's originals and are untouched.

## New mechanics

### Forced-perspective grabbing — `ext/grab.rs`

Superliminal's signature mechanic. It is cheap here because the port kept `p_scale` as a genuine
*physical* scale rather than a rendering trick: it already feeds the transform chain
(`Object.cpp:38`), gravity (`Physical.cpp:21`), walk speed (`Player.cpp:105`) and the collision
epsilon (`Physical.cpp:31`). CodeParade built the hard half of this mechanic without needing it.

An object holds constant apparent size exactly when its size and distance stay in a fixed ratio,
so on pickup we record `k = p_scale / distance` and hold it. Each frame a ray is cast down the
crosshair, and the object is placed just short of the hit so it rests against the surface:

```
d = hit_dist / (1 + r·k)        p_scale = k · d
```

Closed form, no iteration. Three tests pin the invariants — apparent size never drifts, further
placement is genuinely larger, and the object touches the surface rather than clipping it.

### Ray casting — `ext/raycast.rs`

The one genuine gap in the original. `Collider::Collide` (`Collider.cpp:23-45`) only answers
"does this unit sphere overlap that rectangle"; grabbing needs "where along this ray does the
world first block it". Since a `Collider` stores its rectangle as a matrix whose translation is
the centre and whose X/Y axes are the half-extents, this is a plane hit plus two clamped axis
tests — the same projection the collision routine already performs, solved for `t`.

### Observation-dependent geometry — `ext/visibility.rs`

The engine does run occlusion queries, but only for portals and only to prune recursion
(`Engine.cpp:233-251`). Reusing them would mean threading new queries through the ported render
path, so visibility is computed analytically instead: a view-cone test plus a line-of-sight
raycast. That keeps the mechanic entirely inside `ext/`, and gives the answer during `update` —
a frame earlier than the render pass could report it.

### Running — `ext/sprint.rs`

CodeParade's player has one speed. Holding `Shift` raises the `GH_WALK_SPEED` cap to 1.8× and
`GH_WALK_ACCEL` to 1.5×, and quickens the head-bob by 1.35× so the cadence reads as a run
(the bob's amplitude rises with speed as well, by the ported formula — `bob_mag` tracks the
distance covered per step — so a run both strides faster and bounces higher). The multipliers
are exactly `1.0` when not sprinting, so the walk is bit-identical to the port — a test pins
both caps.

Only a forward run sprints: the multipliers apply while the movement vector's forward component
is at least 0.3 of its length (a diagonal is 0.71), so strafing and backpedalling are at walk
speed however the key or button is held. The field-of-view kick follows the run, not the key:
`Shift` held at a standstill does nothing until the player moves forward. And the speed cap
eases rather than steps. `Move`
clips the horizontal velocity to the cap every 2 ms, so a cap that fell from 1.8× to 1.0× in
one step would brake the player at some 1,160 u/s² while the view was still easing back — a
jolt. The cap relaxes with a 100 ms time constant instead, snapping to exactly 1.0 once within a
thousandth so the walk stays bit-exact; acceleration and bob stay stepwise.

The pad gets the console idiom instead of a hold: clicking L3 starts the run, and it ends when
the stick returns to centre or on the next click, so stopping never leaves a toggle armed for
the next push (a click while standing still is ignored for the same reason). L3 held also
works, and a `Shift` press drops a pad toggle, so a player who switches instruments mid-run is
never carried by a toggle they cannot see. The decision is made once per rendered frame, before
the fixed-step loop — `Input::end_frame` zeroes the Shift edge inside it — and written to
`Input::sprint`, which is all the ported player reads.

Running widens the vertical field of view by 8°, eased with a frame-rate-independent exponential
(150 ms time constant) through the same `view::set_fov` the dolly zoom was built for. The pause
menu leaves the kick wherever it was — the world is frozen — and it resumes its ease on the
next played frame; a scene load resets it with the rest of the FOV. Footfalls are counted off
the bob phase (two per cycle) and fire `Sfx::Footstep`, at most once per rendered frame.

A key held across an alt-tab never sees its release, so every key level is dropped when the
window loses focus — a stuck `Shift` would otherwise run the player until it was pressed again.

### Paintings that watch you — `ext/painting.rs`

The Backrooms' entry hall hangs eight portraits whose eyes follow you, built on two things the
engine already did. Every draw receives the eye of the **pass** camera (`RenderCtx.eye`), so the
gaze is computed per render pass: the painting turns that eye into its own canvas metres and the
shader displaces each iris toward it, and a portrait seen through the door looks at the portal
camera -- at the person in the doorway -- rather than at some point on the meadow a thousand units
off. And the visibility test that keeps Level10's statues still while watched
(`ext/visibility.rs`) keeps the sitter's *face* still: only after 0.4 s of nobody looking do the
brows lower, the mouth flatten and the gaze stop following and stare straight out. Look away and
back and it is different; you never catch it moving. The next unobserved stretch puts it back.
The eyes remember, too: while nobody looks they stay aimed at where you were last seen from --
the last sighting, however brief -- and when you look again they slide from there to you over
0.6 s.

The test had to learn to see through a door. From the meadow the hall is a thousand units away
and the plain cone test says "not looking" for every painting in it, which would let them change
while you watched them through the opening. So a painting also counts as observed when its image
in the viewer's world (`Warp::delta`, the transform the portal pass renders with) is in the view
cone, the line to that image crosses the portal's quad, the near half of that line is clear, and
the far half -- from where the line comes out of the far door to the painting -- is clear of the
building. The far half starts at the far door rather than at the warped eye on purpose: a player a
stride outside the meadow door is a stride behind the far door, inside the hall's end wall. A
portal is consulted only for paintings on its far side, by which end of it they are nearer, and
only while the doors stand (`Watch::while_doors_stand`): once the one-way door has gone, so
have its portals, and nothing is seen through them. The line-of-sight raycast sees the
Backrooms' triangle mesh like any other blocker, so a wall is cover.

The portrait itself is procedural (`Shaders/painting.frag`; there is no portrait image in the
asset set): signed-distance shapes for the ground, shoulders, collar, neck, hair, head, eyes,
brows, nose and mouth, a seed picking ground, skin, hair, iris and collar colours and the head's
width, and a brush mottle, canvas weave and faint craquelure over it all, the last two fading
out where their period falls under a couple of pixels. It is lit flat, with the walls' own
squared-distance fog, so it sits in the hall. The frame is four `cube.obj` bars in `gold.bmp`
through the ported `texture` shader, each rolled 45 degrees about its length so what faces the
room is a ridge between two bevels: a moulding that the shader's fixed light models on either
wall, where a flat slat facing away from that light was near black. The canvas is also a
rectangle collider (`Mesh::colliders_only`, `Collider::rect`, nothing drawn from it -- the
quad is drawn through the portrait shader): a ray down the crosshair stops at the picture,
not at the wall behind it, which is what puts a held thing in front of the portrait rather
than behind it. The expression state machine, the gaze memory and the through-the-door test
are unit tested with a fake clock and detached portals.

### The key in the painting — `ext/key.rs`, `ext/painting.rs`, `Shaders/painting.frag`

The last portrait on the hall's north wall, the one by the bare end wall, has a key painted
across the sitter's collar -- in anamorphosis, the way Holbein painted the skull. The key
proper lives on a virtual picture plane through the canvas centre, perpendicular to the line
from a **sweet spot** to that centre, and what is on the canvas is its central projection from
the spot: for every canvas fragment the shader casts the ray from the spot through it, meets
the plane, expresses the hit in the plane's own (u, v) metres and samples a key there (a ring,
a shaft, two teeth, as signed distances). The spot is 2.6 m west along the wall and half a
metre out, at eye height -- `V = (994.4, 1.5, 1.55)` against a canvas centred at
`(997, 1.6, 2.05)`, an eleven-degree look along the wall -- so from anywhere else the key is
a long gold smear, the far end stretched more than the near, and from the spot it closes into
a 9 cm key lying sideways. (Nine centimetres because that grazing view sees only the part of
the canvas between the far edge and the near upright of the frame; the key sits a centimetre
toward the far end to fit.) `painting.rs` mirrors the sum on the CPU (`to_plane`,
`to_canvas`) and the tests check a plane point comes back to itself through the canvas and
that a circle on the plane paints as a smear more than four times wider than it is tall.

Stand in the spot -- within 0.45 m of it, looking within 25 degrees of the canvas centre
(`KeySpec`, `armed`) -- and the paint catches the light, the HUD says TAKE THE KEY, and a real
key (`ext::key::Key`, built at load by the painting and kept) is spawned on the canvas at the
painted key's spot (`room::request_spawn`) and comes out of it over half a second, growing
from nothing as the painted one fades (`Emergence`; the `key_state` uniform is the same
blend). It lies in the picture plane facing the spot and comes out toward the spot, along
the line of sight, as far as it takes to clear the wall by seven centimetres -- not straight
out along the wall's normal: from that grazing view a key ten centimetres off the wall is
seen against the wall seventy centimetres further along, past the frame, and the grab's
placement ray cast through it would put the taken key there, behind the frame's upright.
Along the line of sight it stays over the spot where it was painted, the ray through it
hits the canvas (a collider), and the taken key is held just off the picture, at the size it
was. Step out of the spot before
taking it and it sinks back and is painted again (`room::request_remove`); take it -- it is a
grabbable, one hit sphere, the engine's own gravity and collision once it is loose; its mesh
is `Meshes/key.obj`, extruded by `tools/gen_key.py` from the same numbers as the shader's
distance field, so the key that comes out is the key that was painted -- and its `on_grab`
sets a flag the painting shares, and the painting's key is gone for good. The
emergence and the arm test are unit tested with scripted positions.

Using it: each fixed step the held key looks down the crosshair for the nearest object that
answers `ObjectT::accepts_key` within 2.5 m, by bounding sphere as the grab picks, and offers
E  USE THE KEY; the press raises `room::request_unlock_window`, which the window takes. That
E is also the grab's release, and a used key asks to be removed from its `on_release`, so
the removal lands on the next frame rather than in the step that used it -- a key removed
mid-frame would leave the grab holding nothing by the time it sees the press, and the release
would become a pickup of whatever is under the crosshair. The key's prompt has to beat the
window's own LOCKED, which the grab sets once per frame after the fixed steps, so it goes
through `hint::insist`, the one line that outranks a `set`. Objects see the scene through
`UpdateCtx::scene` for this; the window is built after the paintings, so a snapshot taken at
the painting's load could not have held it.

The 3D key is drawn with the ported `texture` shader, whose one light is fixed from above and
+z; facing the spot squarely it would come out of the canvas near black, so it is pitched a
quarter of a right angle about its length toward the light. A prop look with a real light
replaces that.

### Per-frame room logic — `ext/room.rs`

The ported `Scene` trait has exactly one method, `Load` (`Scene.h:7-9`) — scenes build objects
and then have no further say. Rather than change that trait, rooms needing behaviour push a
`RoomLogic` object: an ordinary `ObjectT` with no mesh and no shader, so `Object::Draw` skips it
(`Object.cpp:21` only draws when both are present) while `Engine::Update` still ticks it.

The one thing such logic cannot reach is the player — `Load` receives `&mut Player`, never the
`Rc` the engine keeps it in — so a room that needs to move them (the Backrooms, when they have
fallen under its floor) calls `request_respawn`, and the engine applies it at the end of the
same step, after the portal pass, through `set_position`: `prev_pos` moves with `pos`, so the
next step's `try_portal` sees no segment that could sweep a doorway. Two more requests ride the
same channel: `request_remove_portals` (the Backrooms' one-way door), applied at the same
point, and `request_scene_load` (the elevator's ride), applied by `run_frame` once the
fixed-step loop is over -- a load replaces the object vector a step is iterating.

### Shared channels — `ext/hint.rs`, `ext/room.rs`

Three more ambient channels, for things in the scene that cannot see each other or the engine.
**The hint** (`hint::set`, `hint::take`) is the one HUD prompt line: whoever offers an action
sets it every frame the offer stands, the overlay block takes it once per rendered frame and
draws it through `hud::draw_hint`, and the last writer before the take wins -- so a prompt
that stops being set vanishes on its own, and nothing is cleared on a scene load. The
elevator's "E  RIDE TO ..." and a grabbable's `pick_hint` go through it. **Spawn and remove**
(`room::request_spawn`, `room::request_remove`) add an object to the scene or take one out of
it -- a key that has been used, a prop a puzzle conjures -- applied by `Engine::update` after
the portal pass, never while a pass is walking the vector; removal is by `Rc::ptr_eq`, and the
indices it frees go to `grab::GrabState::on_removed`, since the grab holds one across frames.
**The unlock** (`room::request_unlock_window`, `room::take_unlock_window`) is a one-shot flag
the key raises and the window consumes, in either order within a step. The hint has a second
slot, `hint::insist`, that the take prefers over a `set` whichever was written first: the
held key's E  USE THE KEY over the window's LOCKED, which the grab writes later in the frame.

### Real physics — `ext/physics.rs`, `ext/rigid.rs`

The ported engine has one kind of motion: a `Physical` falls under its own gravity and is
pushed out of rectangles and triangle meshes sphere by sphere (`Engine.cpp:155-192`). That is
everything a walking player needs and nothing a thrown die needs -- no rotation, no friction
worth the name, no body ever at rest on another. So the things on the Backrooms' carpet -- an
apple, a die, a chess king, a few steps in from the door -- are **rigid bodies** in a
[rapier3d](https://rapier.rs) world (`ext/physics.rs`), and the engine's own passes leave them
alone: `ObjectT::engine_collision` is false for a `RigidProp` (`ext/rigid.rs`), so the
collision pass never pushes it and the portal pass never warps it. Every fixed step the prop
copies its body's pose into `Object::pos` and `Object::rot` -- the rotation-matrix override
the prop scaffolding added, since a tumbling body's orientation does not survive a trip
through Euler angles -- and is drawn through `Object::draw_impl` like any ported object, with
`Shaders/prop.*`: the interior hemisphere `gltfpbr.frag` lights the elevator with, and the
walls' own fog, when the scene is an interior; the ported sun term anywhere else.

**What rapier owns and what the port keeps.** Rapier owns the props' bodies and nothing else.
The player stays on the ported physics -- the walk, the head bob, the portal warp and the
collision epsilon all hang off it -- and is mirrored into the world as a **kinematic capsule**
spanning the player's two hit spheres (`Player.cpp:9-10`), moved to the player's eye every
step, so a prop on the carpet is shoved aside by someone walking into it while the player never
feels the prop (a jump of more than half a metre in one step -- a respawn, a portal, `--pos` --
puts the capsule there rather than sweeping it, which would fling everything on the line). The
static world is rebuilt on every scene load (`Engine::load_scene`, once the object list is
complete) from what the scene's objects already declare for the ported pass: every
`ObjectT::trimesh()` as a fixed triangle mesh -- the collider's own `parry3d` mesh, shared, not
rebuilt; `TriMeshCollider::new` builds it with `FIX_INTERNAL_EDGES` so a body rolling across a
floor of many triangles is not bumped at every shared edge -- and every rectangle collider as a
thin fixed box, except on meshes carrying more than 1,024 of them, which are terrain shells
(the meadow tiles carry 4,356 each, nine of them a load) for ground no prop reaches. It is a
snapshot of what the objects answer at load: the elevator's shut leaves, offered to the ported
pass only while the doors are closing, are not in it, so a prop left in the doorway as they
close is not pushed by them. A prop that leaves the building through a gap in the scan falls past the carpet
for ever, cheaply; the fence's boxes keep it from going far, and nothing catches it.

**Step rate.** The world steps once per engine fixed step, 500 Hz, `dt = GH_DT`, from
`Engine::update` between the collision pass and the portal pass. Measured with the three props
and the 70k-triangle scan: **3-6 µs a step** with the props asleep, **9-12 µs** while they
roll, a worst step of a third of a millisecond (the `[phys]` line at shot time: average, worst
and count since the last shot). Substepping -- k engine steps per physics step -- would buy
nothing worth its complexity at those numbers, and the static world costs the load nothing
because the mesh is shared (`[phys] static world: ... in 0.0 ms`, at debug level).

**Being grabbed.** A `RigidProp` is a `Physical` with exactly one hit sphere, which is how
`ext/grab.rs` recognises a grabbable, so it is picked up, carried, resized by perspective and
put down like the bunny. `on_grab` makes the body kinematic -- it follows the hand and shoves
what it meets -- and turns its rotation matrix back into the engine's Euler order
(`Matrix4::to_euler`, the inverse of `Object::local_to_world`'s `rot_y * rot_x * rot_z`, exact
at the gimbal lock, round-trip tested) so the rotate-with-R1/RMB feature keeps adding to
`euler`. `on_release` freezes the Euler product into `rot`, hands the body back to gravity
with the hand's velocity over the last rendered frame (capped at 12 u/s) and a tumble about
the axis across the throw: let go while the view is moving and it flies. `on_rescale` rebuilds
the collider at the new `p_scale`; mass follows volume through the material's density.

**Adding a prop** is one constructor in a level's `load`: `RigidProp::new(res, name, "x.obj",
"x.bmp", shape, material, pos)`, with a `Shape` (`Ball`, `Cuboid`, `RoundCuboid`, `Cylinder`,
`Capsule`, in metres at `p_scale == 1`; the last two stand on the mesh's origin, the others
are centred on it) and a `Material` (friction, restitution, density). The three shipped props
and their textures are generated by `tools/gen_props.py` (numpy + Pillow): two lathes with
enough segments that the loader's flat normals do not show, a 24-vertex die whose faces map
to a 3 x 2 pip atlas with opposite faces summing to seven, and 24-bit BMPs in the loader's
bottom-first convention.

```sh
# the three at rest, asleep within 240 frames; a [prop] line each, a [phys] line for the step
daydreams --windowed --mute --scene 16 --pos 999.3,1.5,0 --yaw 90 --pitch -30 --frames 240 --shot rest.bmp
# dropped from a metre, tipped past a die's corner: the king topples, the die rolls, all settle
daydreams --windowed --mute --scene 16 --pos 999.3,1.5,0 --yaw 90 --pitch -30 --frames 240 --drop-props 1 --shot drop.bmp
# walk into the apple: it is pushed a metre and a half down the hall
daydreams --windowed --mute --scene 16 --pos 998.5,1.5,0.6 --yaw 90 --forward --frames 120 --shot push.bmp
```

`--drop-props H` is hidden dev tooling: with `--scene`, every prop is lifted by `H` metres
and tipped once the scene has loaded. The world is a thread-local reached through
`physics::with`, for the same reason the elevator's ride and the HUD hint are: a prop is built
inside `Scene::load`, which cannot see the engine, registers its body there, and unregisters
it in `Drop` wherever the object vector lets go of it -- a scene load, a `room::request_remove`.
The engine itself only calls `rebuild_static` and `step`. The pure logic is tested without a
GL context: a ball dropped on a trimesh floor settles at `y = r` and sleeps, a standing
cylinder rests on its base, a kinematic body holds its pose and leaves with the velocity it is
released with, a rescaled ball's mass grows eightfold for twice the radius, the capsule pushes
a ball it walks into and sweeps nothing on a teleport, rectangle colliders become boxes a ball
rests on, and `to_euler` round-trips four hundred rotations including a whisker off the lock.

## Gamepad — `ext/gamepad.rs`

CodeParade *registered* joystick and gamepad raw-input devices in `Engine::SetupInputs`
(`Engine.cpp:454-465`) and left three `//TODO:` stubs behind (`Input.cpp:43-45`,
`Input.h:23-30`). The handler was never written. This finishes it, via `gilrs` and the SDL
controller database — so a DualSense maps correctly over USB or Bluetooth with no
device-specific code.

The analog stick needed only a two-line change to the ported movement code, because
`Player::Move` already normalises the input vector *only* when its magnitude exceeds 1
(`Player.cpp:92-96`). Stick and keyboard simply add and clamp correctly. The new fields went
exactly where `Input.h:23` says `//Joystick //TODO:`.

The same table is in the game, under **Options → Controls**, alongside the keyboard column.

| Control | Action |
|---------|--------|
| Left stick | Move (analog) |
| L3 (stick click) | Run: press to start; ends when the stick returns to centre or on the next press. Holding it runs too |
| Right stick | Look |
| Cross / Square / R2 | Grab / release |
| R1 (hold) | Rotate the held object with the right stick |
| D-pad ←→ | Previous / next scene (in a menu: change the setting under the cursor) |
| D-pad ↑↓, Cross, Circle | Menu: move, confirm, back |
| Options | Open the pause menu / close it again |
| Create | Mute |
| PS button | Fullscreen |

Options and PS both moved. Without a pause binding a pad could start a game and never leave it —
`menu_back` only reaches a menu that is already open, and nothing else on the pad opened one. That
freed quitting to live where it belongs (pause → MAIN MENU → EXIT) and got it off the PS button,
which is also the button you press to wake a sleeping DualSense: an unconfirmed instant exit on the
wake button is a trap.

One detail worth knowing if a controller ever seems invisible: a pad already connected when the
process starts is **not** in `gilrs.gamepads()` yet. gilrs learns of it from a queued `Connected`
event, so anything that counts pads before draining that queue reports zero for a controller sitting
right there. `examples/pad_probe.rs` dumps the raw sequence, the SDL mapping and a live axis stream:

```bash
cargo run --release --example pad_probe
```

## Audio — `ext/audio.rs`

The original engine is completely silent. Built on `kira`, chosen over `rodio` because scene
switching wants real crossfades.

**Music** — drop `ogg`/`mp3`/`wav`/`flac` into `assets/music/`. A numeric filename prefix binds
a track to a scene (`03-pillars.ogg` → scene 3); anything else becomes the fallback for scenes
without their own track. Scene changes crossfade.

Music is **streamed**; effects are decoded up front. `StaticSoundData` holds a whole track as f32
samples, which for the eight-minute soundtrack is ~275 MB resident and a third of a second of stall
at the scene load that starts it. A streaming sound decodes ahead on kira's thread instead, so a
track costs about the same whether it runs one minute or twenty. The trade is that a stream can fail
*during* playback where a static sound cannot, so `Audio::tick` drains the handle's error queue every
frame rather than letting the music stop with no explanation.

**Sound effects** — drop files into `assets/sfx/` named `grab`, `release`, `portal`, `land`,
`footstep` or `elevator`. `grab`, `release`, `footstep` and `elevator` (the doors closing on a
ride) have call sites; `portal` and `land` are loadable but nothing fires them yet. None of the
files ship.

Everything degrades to a no-op. No audio device, no `assets/` directory, or no files, and the
engine still starts and runs silently — a demo should not refuse to launch over a missing sound
file, and the ported engine has no error path to surface one through.

## Settings — `ext/settings.rs`

**Options** carries mouse sensitivity, gamepad sensitivity, mute, and a link to the **Controls**
key map. Up/down moves between rows, left/right changes the row you are on — D-pad included, so
the whole screen is reachable from the controller.

Sensitivity is an integer notch 1–10 rather than a float, because ten labelled stops are
navigable with a D-pad in a way a continuous value is not. Notch 5 is exactly 1.0×, so a player
who never opens the menu gets the ported feel bit for bit; the ladder is geometric (×1.25 a
notch, 0.41× to 3.05×), so each stop changes the feel by the same *proportion*. Mouse and pad
carry separate notches — a stick is a rate control and a mouse a displacement one, and a player
who uses both wants two numbers rather than one compromise.

Values live in `settings.toml` in the per-user config directory, resolved by the `directories`
crate (`ProjectDirs::from("", "", "DayDreams").config_dir()`):

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/DayDreams/settings.toml` |
| Linux | `$XDG_CONFIG_HOME/daydreams/settings.toml`, normally `~/.config/daydreams/settings.toml` |
| Windows | `%APPDATA%\DayDreams\config\settings.toml` |

Not beside the binary and not in the working directory, because neither is writable from a
signed `.app` or a Finder launch. The directory is created on the first write. The format is
three TOML keys, with a comment on top:

```toml
# DayDreams settings. Sensitivity is a notch from 1 to 10; 5 is the default.
mouse_sensitivity = 5
pad_sensitivity = 5
muted = false
```

A `serde` struct with `#[serde(default)]` reads it, so any key may be missing; an out-of-range
notch clamps, and unknown keys are ignored — an older build reads a newer build's file. The
leniency has a shape worth knowing: a wrong-typed value costs that field (each deserialises
through `toml::Value`, so `mouse_sensitivity = "high"` keeps that one default); a file that is
not valid TOML costs all of them, with one warning in the log. The old parser skipped bad
lines, so a file it would have half-read is now either read or not.
The previous format, `settings.cfg` in the working directory with `muted = 1`, is valid TOML and
still loads; if the new file does not exist and `./settings.cfg` does, it is read once and the new
file written from it, after which the old one is ignored.

The file is written at most once a frame and only when something changed (the menu marks a flag;
the engine flushes it), so a held D-pad direction never puts a filesystem write between the player
and the value they are aiming for. Everything degrades to a no-op: an unreadable file leaves the
defaults, an unwritable one loses the preference rather than interrupting the game about it, and a
machine with no home directory runs on defaults and says so once.

`GH_MOUSE_SENSITIVITY` is a compile-time constant read straight out of `Player::Look`
(`Player.cpp:74,82`), so the runtime multiplier is a thread-local read at the two call sites —
the same shape as `ext/view.rs`'s runtime FOV, and for the same reason: ported code has no path
to the extension state.

## Load time and frame cost

Measured on an M3 Max at 3456×2168, scene 15 (the intro meadow, which is the heaviest thing the
game loads and what the title screen sits in front of). Frame times are with `--no-vsync`, the
average over 590 frames after the scene settled; "reload" is the title → NEW GAME transition,
which loads the very scene already on screen.

| | Before | After |
|-------|--------|-------|
| Title screen frame | 4.94 ms (p95 7.3) | **2.28 ms** (p95 3.6) |
| Spawn, facing the door (portal in view) | 4.22 ms | **1.77 ms** |
| Spawn, facing away (no portal in view) | 3.60 ms | **1.13 ms** |
| Scene load (`Engine::load_scene`) | 0.86 s warm, 1.45 s cold | **34 ms** |
| Reload (title → NEW GAME, MAIN MENU, RESTART) | 0.72–0.83 s | **< 1 ms** |
| Process start to first frame | 1.46 s | **0.62 s** |
| Peak resident | 1107 MB | **269 MB** |
| Assets on disk | 124 MB OBJ + 79 MB GLB | 1.3 MB GLB |

Where it went, in order of effect:

* **The occlusion query readback was the frame.** The ported renderer draws every portal's
  quad inside a `GL_SAMPLES_PASSED` query and reads the result back *in the same pass* to
  decide whether to recurse (Engine.cpp:236-247). That read is a full CPU–GPU round trip: the
  driver has to finish everything queued so far before it can answer, about 2 ms here — and it
  was paid again inside the nested portal pass, for a portal a thousand units behind the far
  plane. Now a portal whose quad lies wholly outside the pass's frustum is settled on the CPU
  (it would pass no samples either way, so the answer is identical), and a pass with no
  portal in view skips the query block altogether. Facing away from the door, no pass stalls.
* **The readback that remained is one frame late** (`src/ext/occlusion.rs`). The queries are
  still issued every pass, each stamped with the frame it was issued on, and the answer used
  is the one the same slot produced on the *previous frame exactly*, read with
  `GL_QUERY_RESULT_AVAILABLE` first. A portal is hidden only when that one-frame-old result
  is available and counted zero samples; in every other case -- never issued, issued two or
  more frames ago (its portal was out of the frustum since, so the slot was not asked), or
  not yet available -- it counts as visible (`SlotPolicy::decide`, unit-tested). The stamp is
  what stops a slot handing back a "0 samples" from some frames-old view the moment its portal
  re-enters the frame. No pass ever stalls. A slot is one portal seen from one pass, and a
  pass is named by the chain of portals it is seen through, so the same portal seen through
  two different portals keeps two answers. What this changes on screen: a portal that becomes
  fully hidden is drawn for one extra frame (it is hidden, so nothing shows), and a portal
  uncovered after being fully hidden is first drawn on the frame *after* it appears -- for
  that one frame the uncovered sliver, one frame's motion wide, shows what was drawn behind
  the quad. Portals entering from outside the frustum are unaffected: the CPU pre-test above
  settles those in the current frame, and their slot's last result is older than a frame.
* **Each portal pass is scissored to the quad's screen footprint** (`src/ext/scissor.rs`).
  `portal.frag` samples the nested framebuffer by screen-space projection, so only the quad's
  footprint is ever read; the quad's corners are projected, their NDC box is clamped and
  padded by two pixels, and the nested `Engine::render` -- its depth clear included -- runs
  under that scissor. A quad with a corner at or behind the eye gets the full viewport.
* **The sky is drawn last**, at the far plane under `GL_LEQUAL`, so it shades exactly the
  pixels nothing else covered (the holes a `discard` leaves included). The port drew it first
  and let the scene overdraw about half of it, in every pass.
* **One portal framebuffer per recursion level, sized to the drawable.** The port gave every
  Portal its own three 2048-square framebuffers (~20 MB each: 360 MB for the floorplan, a
  gigabyte for a twelve-portal scene) and under-sampled the door on any drawable wider than
  2048. A portal renders its framebuffer and draws from it before any sibling at the same level
  renders, so `Engine` owns `GH_MAX_RECURSION - 1` of them for all portals, rebuilt on resize
  and capped at `GH_FBO_SIZE` (now 4096) a side.
* **What is left per portal is the render-target round trip itself.** On this machine a
  portal in view costs ~0.9 ms whatever is drawn into its framebuffer: skipping the nested
  draw entirely, drawing it into the main framebuffer instead, clearing first, or shrinking
  the framebuffer to 256 square all measure the same, so the cost is Apple's GL-on-Metal
  layer switching render targets and sampling a texture rendered this frame, not fill or
  readback. Scenes whose portals nest several deep (Six Rooms, 1.48 -> 1.12 ms) are where the
  items above show; the intro and title, one portal in view, are within noise of where they
  were.
* **The blade patch is generated, indexed and culled** (`src/ext/grassgen.rs`,
  `src/ext/grassfield.rs`). The 124 MB `grass_patch.obj` is gone; the same scatter runs in
  20 ms at startup and is pinned for the life of the engine. Ten shared vertices per blade
  instead of the OBJ's de-indexed 48 (every quad twice, once per winding, because the engine
  culls back faces globally — the field's own draw now turns culling off for its duration),
  so 1.3 M vertices and 1.04 M triangles in place of 6.2 M and 2.08 M. Blades are bucketed into
  a 13×13 grid of 2-unit cells with contiguous index ranges, and each pass draws only the cells
  whose box meets its frustum, merged into one `glDrawElements` per run — looking across the
  patch that is about half the blades, for 169 box tests.
* **Frustum culling everywhere else** (`src/ext/cull.rs`): six planes pulled from the
  view-projection once per pass and carried on `RenderCtx` with the eye position. The terrain's
  nine tiles cull by box, every `Object` by bounding sphere, the door by a sphere around its
  foot. The oblique-clipped portal cameras work unchanged: the planes extracted from their
  matrix are exactly the ones the GPU clips against.
* **The door GLB is pre-shrunk** (`tools/shrink_glb.py`): its PNGs were 4096², and the loader
  was decoding ~400 MB of RGBA on three threads to resize them to its 512² `MAP` on every
  load. They are stored at 512² now, resized once with the same filter, and the file went
  from 79 MB to 1.3 MB. The geometry bufferViews are copied byte for byte; the JSON is
  re-serialised with identical values (only the float spellings may differ).
* **The old scene's objects outlive the load.** `Engine::load_scene` used to clear the object
  and portal vectors before `Scene::load`, which expired every `Weak` in the resource caches;
  the title → NEW GAME transition therefore re-parsed the terrain, re-built the grass and
  re-decoded the door. The object vector is moved into a local, the new scene loads against
  warm caches, and the old objects drop afterwards. The portals go the same way: a portal
  owns only its quad and two shaders, which are exactly what the next scene's portals
  re-acquire. (They used to be dropped first, with those three pinned in `ExtState` instead,
  from when each portal carried its own framebuffers -- some 60 MB a portal, which keeping
  twelve of them across the load of twelve more would have doubled. The framebuffers live on
  the engine now, shared per recursion level, so there is nothing left to avoid holding
  twice.)
* Smaller things: `Shader` memoises by-name uniform locations (six lookups per object per pass
  were each a `CString` and a driver call); the eye position is computed once per pass rather
  than inverted per object; the blade vertex shader takes `vp` and `l2w` instead of recovering
  them with two `inverse()` calls per vertex; occlusion queries live for the engine's life; the collision
  pass reuses one scratch vector for hit spheres.

Under vsync — the default — the game is display-limited long before it is GPU-limited, so a
frame measures the refresh period no matter what is in it. Note that on macOS "no vsync" has to
be asked for: CGL's default swap interval is 1, so merely omitting the request leaves the swap
throttled (which is how an early version of the flag measured 11 ms for the title).

## Hooks into ported files

Small additions, each tagged `// EXT:`:

| File | Hook |
|------|------|
| `collider.rs` | read-only `mat()` accessor, so rays can transform the rectangle to world space; `Collider::rect(centre, half_u, half_v)`, the three-corner constructor with the sorting already done |
| `input.rs` | four analog fields, filling the `//Joystick //TODO:` slot; `E`/`M`/`R` and the scene keys `8`–`.` (`8` `9` `0` `-` `=` `[` `]` `\` `;` `'` `,` `.`); `Shift` into the `VK_SHIFT` slot and the resolved sprint multipliers |
| `player.rs` | stick axes added to the keyboard move and look vectors; sprint multipliers on the speed cap, acceleration and bob rate, and a footfall counter |
| `object.rs` | `UpdateCtx` carries the player's eye transform, so room logic can see where you look, and the scene's object vector (`scene`), for an object that reads the others during its step (the held key); `RenderCtx` carries the pass frustum, eye and the shared portal framebuffers, and `draw_impl` culls by bounding sphere; `ObjectT::trimesh()` for triangle-mesh scenery; `Object::rot`, a rotation matrix that stands in for `euler` in `local_to_world`/`world_to_local`/`forward` when set (a rigid body's orientation does not round-trip through Euler angles); the prop hooks on `ObjectT`, all defaulted: `engine_collision()` (false: the collision pass never pushes it and the portal pass never warps it -- something else owns its motion), `on_grab()`, `on_release(velocity)`, `on_rescale(p_scale)` (called by `ext/grab.rs`), `place_flat()` (the grab lays it on the surface it hits instead of standing it off by its sphere), `pick_hint()` (a HUD line while the crosshair is on it) and `accepts_key()` (the held key can be used on it: the window, while locked) |
| `frame_buffer.rs` | sized attachments instead of `GH_FBO_SIZE` square |
| `engine.rs` | one `ext` field, the scene vector, names and keys read from the [registry](#scene-registry), a grab tick, the sprint resolve, scene-load notification; the portal frustum pre-test and the one-frame-late occlusion slots; the old scene's objects and portals kept alive across `load_scene`; the triangle-mesh rounds in the collision pass; a room's respawn, portal-removal, spawn and remove requests applied after the portal pass (a removal's freed indices handed to the grab), its scene-load request applied after the fixed-step loop; the collision pass skipping an `engine_collision() == false` object as its subject and the portal pass skipping it outright; E offered to the elevator before the grab, the frame's hint (`ext/hint.rs`) and the elevator's black-out in the overlay block (the black-out under the pause menu too); the `--forward`/`--strafe`/`--sprint` held keys, `--arrive`, `--ride-at`, `--hold-key` and `--e-at`, handed over as one `cli::DirectRun`; `load_scene_from`, the body of `load_scene` taking a scene that is not in the registry (`--view-glb`); the rigid-body world's static rebuild once a load's object list is complete, its step between the collision and portal passes, its `[phys]`/`[prop]` report at shot time and `--drop-props` (`ext/physics.rs`) |
| `portal.rs` | the nested pass scissored to the quad's screen footprint; `passable` (default true) and `tint` (default clear), the second uploaded to `portal.frag` as `uniform vec4 tint` and mixed over the far side by its alpha |
| `physical.rs` | `try_portal` returns false without warping through a portal that is not `passable` |
| `shader.rs` | memoised by-name uniform lookup (misses cached too), `set_mat4`; `new` returns `Result<_, AssetError>` and the attribute scan is a pure, tested `scrape_attribs` |
| `texture.rs` | `new` returns `Result<_, AssetError>`; the BMP byte walk is a pure, tested `decode_bmp` |
| `mesh.rs` | `new` returns `Result<_, AssetError>`: a missing `.obj` is an error, not an empty mesh; `Mesh::colliders_only(gl, colliders)`, a mesh with no faces and only rectangles -- the in-memory `intro_door_collide.obj` |
| `resources.rs` | every `acquire_*` turns a loader's `Err` into `app::crash::fatal` |
| `props.rs` | `Sky::draw` takes the eye from the inverse it already computes, and draws at the far plane under `GL_LEQUAL` so it can go last |
| `main.rs` | gamepad polling in `about_to_wait`; the platform layer's startup order (panic hook, command line, logging, asset root -- all in `src/app/`); the hidden `--panic-test`; key levels dropped on focus loss; the window's dev preset (`--window-scale`, `--unlock-window`) set before the engine builds a scene |

### One bug this surfaced

`Input::EndFrame` memsets `key_press` to zero (`Input.cpp:11`) and runs **inside** the 500 Hz
fixed-step loop. Anything reading `key_press` after that loop always sees `false`. The ported
scene keys avoid this only by being checked before it. Extension keys are now sampled in the same
place and latched.

## Known limits

**Portals must stay vertical.** `Physical::try_portal` only rewrites `euler.y` on teleport
(`Physical.cpp:57-59`), so passing through a tilted or rolled portal would leave the player
un-reoriented. This rules out floor/ceiling portals, Klein-bottle corridors, and Manifold
Garden-style player gravity flips. Per-*object* gravity is already an arbitrary `Vector3`, so
objects are unaffected.

**No object-vs-object collision.** The ported pass tests each `Physical`'s hit spheres against
other objects' *mesh colliders* (`Engine.cpp:155-192`), so grabbed props collide with level
geometry but pass through each other. They cannot be stacked. The rigid-body props are the
exception ([Real physics](#real-physics--extphysicsrs-extrigidrs)): those collide with each
other, and with nothing on the ported path but the player's capsule.

**No HUD.** There is no crosshair, so aiming a grab is currently guesswork at screen centre.

## Meadow, grass and clouds (scene `\`)

An outdoor scene built to look like the classic rolling-hills-and-cumulus photograph while
costing less than any indoor room -- it renders at roughly 1,100 fps uncapped, against ~190 for
the original demo's portal-recursive first level. Every expensive thing happens offline or once:

| What | Where the cost went |
|------|---------------------|
| Cloud shapes (6-octave domain-warped FBM, single-scatter lighting, cirrus layer) | Baked **once** into a 1536x768 panorama by a GLSL pass (`src/ext/skybake.rs`, `Shaders/cloudbake.*`), re-baked every 6 s with advanced noise time so the field evolves, on the title screen as well as in play. The runtime sky (`Shaders/sky.frag`) is one texture fetch + the original sun term -- which matters because the ported renderer draws the sky inside every portal pass. |
| Grass detail | Baked offline into a tileable noise atlas (`tools/gen_meadow.py` -> `Textures/grass_noise.bmp`). The shader does two or three taps and zero noise math. |
| Rolling hills | One heightfield mesh with smooth normals smuggled through the engine's 3-component `vt` channel (the parser discards `vn`), plus a gradient-tilted collider shell -- same scheme as the Relativity walk shell. |
| "Realism" | Three illusions in `Shaders/grass.frag`: drifting **cloud shadows** (a scrolled low-frequency tap), **valley occlusion** (world height as free AO), and **atmospheric perspective** toward the sky's horizon colour. Plus a backlit sun sheen and a wind ripple that moves no vertices. |

The intro level adds real **grass blades** on top of the textured ground: a 26×26 unit patch of
130,000 blades generated in-process (`src/ext/grassgen.rs`, a port of the Houdini scatter that
used to be shipped as a 124 MB OBJ) and kept centred on the player by snapping its position to a
2-unit grid, so blades never slide underfoot. The patch is indexed, one winding, and bucketed into
cull cells so a pass only draws what it can see; `Shaders/grassblade.vert` does all the bending
and stands each blade on the terrain's height field. See [Load time and frame
cost](#load-time-and-frame-cost) for what that costs.

The new sky applies to every scene; the ported gradient-only sky is kept as
`Shaders/sky_plain.frag.txt`.

## Backrooms (scene `'`)

**NEW GAME starts here**, and the title screen is this level seen from the meadow: the intro
again -- same meadow, same white door, both built by `ext/meadow.rs`, which the two scenes
share along with the far world's origin and the title screen's vantage -- except that through
the door is
`Meshes/backrooms_vr.glb`: a Sketchfab light-bake of the Backrooms, 29 primitives, 70k
triangles, 27 maps, every material `KHR_materials_unlit`. (`scenes::INTRO` is looked up by
name, so the Intro proper -- the sunset sea, scene `;` -- is still in the level list, just no
longer where the game begins.) Three things had to exist for it to be a place rather than a
picture:

| What | Where |
|------|-------|
| **A general glTF load.** The door loader fitted one model to one height and packed PBR maps at 512. `Load { fit, max_map, .. }` now chooses between that and `Fit::Identity` (source metres, source origin -- the scan is already to scale and already Y-up once the root node's rotation is applied), and per-material `unlit` skips the PBR pack entirely: the base map goes up as shipped, at its own size and with its own wrap mode, and `baseColorFactor` / `emissiveFactor x KHR_materials_emissive_strength` are uniforms. A part is drawn with one shader, so its materials must all be unlit or all PBR -- the loader asserts it. The door's path is byte-identical. What the loader has grown since is under [glTF loader](#gltf-loader). | `ext/gltf_model.rs`, `Shaders/gltfunlit.*` |
| **Triangle-mesh collision.** The engine only knew axis-aligned rectangles declared on OBJ `c` lines. The whole scan becomes one parry3d `TriMesh`, built once in world space; the collision pass asks it for the deepest sphere penetration, applies the push exactly as it applies a rectangle's (on_hit, on_collide, matrices rebuilt), and asks again, up to eight rounds. The player walks on the carpet, is stopped by walls, skirting and armchairs, and the grab raycast sees the same surfaces. | `ext/trimesh.rs`, `ObjectT::trimesh` |
| **Frustum culling.** The model is drawn by the meadow's main pass too, 1,000 units away. A six-plane test from the pass camera's matrix (oblique near plane included) skips the draw when the part's bounding sphere is wholly outside. | `ext/cull.rs` |

The model is placed by its door: `ext/backrooms.rs` names a spot of open carpet in model
coordinates (`DOOR_SPOT`, in the 23 m hall at the building's east end -- 3.6 m clear between
its wall faces at z = 3.48 and 7.07 -- 1.2 m clear of the end wall) and `Backrooms::new` takes
the world point it should land on, so the level reasons about the return door and the placement
follows. A test measures those faces from the GLB itself (`GltfModel::probe_triangles`, no GL
context needed) and checks the door's frame posts clear them by a player's width, so the
constant cannot be nudged into a wall. An invisible fence one metre outside the model's extent
keeps the player inside whatever the scan's seams allow; under the carpet there is nothing, on
purpose. The walls are single-sided and the collider keeps a sphere on whichever side its centre
is on, so a sphere caught inside a wall slab can be pushed out the far side, where there is no
floor -- a net there left the player standing in the dark for ever, so instead a `RoomLogic`
(`ext/backrooms.rs::fell_out`, `ext/room.rs::Respawn`) puts anyone half a metre under the carpet
back at the arrival point, facing down the hall. The backrooms shader ignores the weather grade
every other surface takes -- its lighting is painted in -- and adds a squared-distance fog toward
dark yellow-brown so the far end of the maze fades rather than popping at the 100-unit far
plane. What does take the grade past the split -- the return door's paint, the sky through a
window pane or a gap in the scan's single-sided walls -- is told the far world is an
**interior** (`view::set_far_mood`, `MOOD_INTERIOR`): the door stays white instead of
sunset-pink, the sky is the near-black of an unlit building going on past its walls, and a dark
ground cap under the whole footprint (`backrooms::GroundCap`, colliding with nothing) makes the
void below the horizon the same darkness.

Along the hall's walls hang eight portraits (`ext/painting.rs`, [above](#paintings-that-watch-you--extpaintingrs)):
five on the north wall, three on the south, each 0.8 x 1.0 m with its centre at 1.6 m and its
back two centimetres off the scan's wall face so the two never z-fight. Their eyes follow
whichever camera is drawing them, door included, and their faces change only while nobody is
looking. Eight of them add nothing measurable to the frame: with vsync off the hall views stay
at 1.0 ms and the meadow spawn (the door drawing the hall) within noise of its 2.2 ms. Between
the second and third of the north wall's portraits hangs [the window](#the-window). The last
one on the north wall wears [the key](#the-key-in-the-painting--extkeyrs-extpaintingrs-shaderspaintingfrag).

Frame cost, measured with `glFinish` after each frame on a shared M3 Max at 2560x1440 (so
absolute numbers are pessimistic; the comparison is what matters): meadow spawn in the intro
**8.1 ms**, the same spawn in the Backrooms (the portal now draws the hall) **8.6 ms**, standing
in the hall looking down it **6.1 ms**, looking back at the door (the portal draws the meadow)
**8.0 ms**. Physics at 500 Hz with the triangle collider is inside those numbers.

**The door is one way.** Step through it and, on the first step you take on the carpet, both
doors and both portals are gone for good: the meadow door and the one on the carpet share a
`DoorLink`, and the link carries a vanish flag (`DoorLink::vanish`) that makes each frame stop
drawing, stop colliding (its collider proxy is dropped) and stop glowing, and ignore every
reason to open -- proximity, the title's hold-open, its partner. The portals are removed
through the engine (`room::request_remove_portals`, applied after the portal pass like the
respawn, since that pass is what just warped you), because a door that is gone must not leave a
hole in the air that still leads somewhere. The trigger is `meadow::in_far_world`: past the
mood split, which -- the warp being instantaneous and the two worlds a kilometre apart -- is the
same fact as having crossed. Arriving by elevator counts too. The title screen's camera is
parked on the meadow and never crosses, so the backdrop keeps its open door and the hall
through it. What is left at the hall's east end is its bare wall; the meadow stays loaded, a
thousand units off and culled, for the title to come back to. The way on is the
[elevator](#elevator), at the dead end of the entrance corridor south of the hall.

## Elevator

`ext/elevator.rs` is the hub between the game's interiors: EFX's *Elevator with Animation
LOWPOLY* (`Meshes/elevator_with_animation_lowpoly.glb`, CC-BY-4.0, `THIRD_PARTY.md`) -- a
cabin, a wallpapered wall slab around its doorway with a call button and a floor display, and
two telescoping leaves the file's one clip, `Doors open`, slides open and shut. Stand in it and
press E.

**How it works.** The ride is a small state machine (`Ride`, GL-free, tested with a fake
clock): `Idle` (doors open) -> E inside the cabin -> `Closing`, the leaves slide shut over 1.5
s -> `Fading`, a black overlay comes up over 0.5 s -> `room::request_scene_load`, applied by
the engine after the fixed-step loop, never mid-step -> the destination level takes the
`Arrival` with `elevator::take_arrival()`, builds its own elevator with it (doors shut, screen
black) and stands the player in the cabin with `Elevator::board` -> `Arriving`, the black clears
over 0.5 s and the doors open over 1.5 s -> `Idle`. E is the grab key; standing in an idle
cabin it rides instead, and the HUD says `E  RIDE TO <floor>` -- or `NO OTHER FLOORS`, when
nothing else is registered. The doors are drawn as their own parts at the clip's `node_delta`
for a clip time of `openness x T_OPEN`, where `T_OPEN` is the clip's widest moment, found once
at load by sampling it, so closing is the opening curve played backwards. Collision is two
triangle meshes: the cabin (floor, sill, walls, ceiling, slab) always; the shut leaves on a
helper object (`ElevatorDoors`) offered only while the doors are less than half open. The
fade, the E press, the ride-start cue for `Sfx::Elevator` and the arrival are ambient
channels (thread-locals, as `ext::view`'s uniforms are), because nothing in the object vector
survives the load and nothing in it can reach the engine's HUD or input; the hint goes through
the shared line every prompt uses (`ext/hint.rs`), set each step the offer stands.

**Floors.** `elevator::FLOORS` lists the levels an elevator stops at, in riding order, by their
registry name (`scenes::index_of`); a floor whose scene is not registered is skipped with one
warning, so the list can name a level before it exists. From any floor the ride goes to the next
in the list that resolves, wrapping round.

| Floor | Scene | Label |
|-------|-------|-------|
| 0 | Backrooms | `BACKROOMS` |
| 1 | Pool Rooms | `POOL ROOMS` |
| 2 | Overgrown | `OVERGROWN` |

**How a level adds one.** The elevator first, then the model it is set into, because the
model is carved round it as it loads:

```rust
let arrived = elevator::take_arrival();                  // Some(..) when a ride brought us
let lift = Elevator::new(gl, res, threshold, door::yaw_facing(facing), arrived);
let rooms = Backrooms::new(gl, res, FAR, &[lift.wall_cut()]); // or, for a glTF interior,
// interior::load(.., Openings { cut: &[lift.wall_cut()], also_inside: &[lift.world_bounds()] }, ..)
if arrived.is_some() { lift.board(player); }             // in the cabin, facing the doors
// fence in lift.world_bounds() along with the room's, then
objs.push(lift.doors()); objs.push(lift);
```

`threshold` is the centre of the doorway at floor level on the outer face of the wall slab,
`elevator::PROUD` (3 cm) in front of the host wall's face so the two are never coplanar; the
yaw is the one `door::yaw_facing` gives for the direction the doorway faces (local +z, as a
`Door`). The cabin is 2.3 m deep behind the slab and the slab 4.2 m wide, the doorway a metre
east of its centre (`elevator::SLAB_X`, `OPENING_X`, `THRESHOLD`, all measured from the GLB by a
test). `wall_cut()` is the world-space box the host carves out of its model -- everything
behind the slab's face that the elevator occupies, the slab's width, from a hand under the
cabin's floor to the slab's top -- and the carving is the glTF loader's (`Load::cut_boxes`,
`ext/carve.rs`): as the file is parsed every triangle crossing the box is clipped so exactly
the part outside survives, its UVs, normals and tangents interpolated along the cut, and the
collider is built from the same triangles. The host's wall is genuinely absent behind the
doorway -- and so is a thick wall's inside or a floor where the cabin now stands -- with no
depth-buffer trick that a wall in front of the doorway could fall through. A world box stays
a box in the model's space only because every interior is placed by translation and quarter
turns (`bounds::model_box` asserts it). In the Backrooms the slab stands in the end wall of
the entrance corridor, centred in it so a hand's width of the scan's own wall shows either
side, its doorway east of the corridor's centre line to clear the armchair the scan parks
against that wall; `level16::ELEVATOR_SPOT` is derived from the corridor's and the slab's
extents, and a test measures the wall, the ceiling and the chair from the scan. In the Pool
Rooms it is set into the hall's west wall at mid-length, in the Overgrown room into the south
wall (`ELEVATOR_SPOT` / `ELEVATOR_YAW` in `level17.rs` and `level18.rs`; the sections below).

**Testing a ride without a hand on the keyboard.** Two hidden flags: `--scene N --arrive`
loads a scene as a ride would -- black, doors shut, stood in the cabin facing its doors (the
look is the cabin's unless `--yaw` or `--pitch` is given) -- for photographing an arrival;
`--ride-at FRAME` presses E once on that rendered frame, exactly as the key would be pressed:
in an idle cabin it rides, anywhere else it grabs, which is to say nothing happens. A ride
takes about 4 s of wall-clock time (1.5 s closing, 0.5 s fade, the load, 0.5 s fade, 1.5 s
opening), and frames run at the display's refresh under `--windowed` (8.4 ms each on a 120 Hz
panel), so `--frames` has to leave room: 600 frames is 5 s there and 10 s on a 60 Hz panel,
and waiting longer in an idle cabin changes nothing. The three rides, each printing the far
floor's `[load]` line and photographing it through the open doors from inside the cabin
(`--pos` is the cabin's middle, `--yaw` faces its doors):

```sh
# Backrooms -> Pool Rooms: expect `[load] scene 17`, the flooded hall through the doors
daydreams --windowed --scene 16 --pos 995.23,1.5,-8.25 --yaw 180 --ride-at 30 --frames 600 --shot ride1.bmp
# Pool Rooms -> Overgrown: expect `[load] scene 18`, the moss and grass through the doors
daydreams --windowed --scene 17 --pos 0,1.5,3 --ride-at 30 --frames 600 --shot ride2.bmp
# Overgrown -> Backrooms: expect `[load] scene 16`, the entrance corridor through the doors
daydreams --windowed --scene 18 --pos 0,1.5,3 --ride-at 30 --frames 600 --shot ride3.bmp
# The same first ride from an arrival: E lands after the 2 s arrival, not during it
daydreams --windowed --scene 16 --arrive --ride-at 300 --frames 900 --shot ride1b.bmp
# E ignored: outside the cabin (position and scene unchanged), and while the doors are moving
daydreams --windowed --scene 17 --ride-at 30 --frames 300 --shot none.bmp
daydreams --windowed --scene 16 --arrive --ride-at 30 --frames 300 --shot none.bmp
```

The `[shot]` line's `player at` shows the boarding: `(0.00, 1.50, 2.89)` in the two glTF
interiors (the cabin's middle behind a doorway at `z = 1.5`), `(995.25, 1.50, -8.25)` in the
Backrooms' corridor.

## Pool Rooms and Overgrown (scenes `,` and `.`)

Two of the three Sketchfab assets the loader grew for (the third is the [elevator](#elevator))
as levels of their own: Blenderust's "Level 37 flooded tiled complex"
(`Meshes/level_37_flooded_tiled_complex.glb`) and "Backrooms room with plants, overgrown"
(`Meshes/backrooms_room_with_plants_overgrown.glb`), both CC-BY-4.0, licences beside them and
credit lines in `THIRD_PARTY.md`. Neither has a meadow in front of it: the player is simply
stood inside, and the whole scene is graded as an interior from the first step
(`view::set_scene_mood`). What the Backrooms built by hand for one unlit scan -- model, triangle
collider, fence, ground cap, fall-out respawn -- is a function now, `ext/interior.rs`, and each
level is a `Load`, a placement and a handful of constants measured from the file, with tests
that measure them again (`GltfModel::probe_solid_triangles`, rays cast against the placed
triangles: the floor under the arrival is at y = 0, the ceiling is where the docs say, the wall
behind the reserved elevator spot is flat for the width of a cabin and clear in front). The
collider is built from the **solid** triangles only (`GltfModel::solid_triangles`): nothing
alpha-tested or translucent collides, so the pool's water is waded through and the foliage
cards are walked through rather than bumped into at their transparent corners.

Each level places its model so that its arrival spot is the world origin with the floor at
y = 0 and the player facing -z, the engine's default heading, which keeps `--yaw 0` meaning
"straight ahead" here as everywhere else. The arrival is 1.5 m in front of a stretch of outer
wall reserved for an elevator, exported as `ELEVATOR_SPOT` (the cabin's floor point on the
wall's inner face, world coordinates: `(0, 0, 1.5)` in both levels) and `ELEVATOR_YAW` (the
yaw its doorway faces, `Object::euler.y` convention: pi, i.e. -z); the level's own spawn is
derived from the two (`interior::arrival`), so they cannot drift apart. Each level builds its
elevator first, its slab `elevator::PROUD` of the wall, and hands `interior::load` the
`Openings` it makes: the cabin's `wall_cut()`, carved out of the model as it loads (the
hall's wall behind the doorway, and for the pool the inside of its metre-thick wall where the
cabin now is), and its `world_bounds()`, which the fence encloses along with the model -- the
2.7 m cabin sinks through the wall and stands outside the building, behind it. A ray cast
from the spawn into the cabin in either level finds the cabin's back wall and nothing of the
host; `--forward` from the spawn at `--yaw 180` walks in and stops at it (z = 3.85).

**Pool Rooms.** The model is two storeys of white mosaic tile, 37 x 33 m, with marble slides
through both and a tiled basement under the floor. The level is the lower hall: a flat floor
at model y = 0 with no holes (the basement is sealed under it), inner wall faces at model
x = -3.15 and 27.62, z = -22.13 and 3.3 -- 31 x 25 m of pillars on a 6 m grid -- under the
upper floor's slab at 4.15 with ceiling lamps hanging to 3.64. The water (`Water.002`, named
in `Load::translucent`, a constant 75% alpha) lies at model y = **0.78**: knee-deep wading, the
eye at 1.5 staying 0.72 m above the surface. The upper storey (floor 4.2, its own water at
4.54) is reached only by a spiral stair in the hall's south-west corner, and the engine cannot
climb stairs -- the foot sphere stops at every riser, which is why `level13.rs` blurs
Relativity's staircases into ramps -- so it and the slides, which leave the building through
the south wall, are scenery. The hall's long axis is model +x, so the model is turned
a quarter turn (model +x to world -z) and the elevator spot is on the west wall's inner face
at mid-length, centred between two pillar rows 2.4 m out -- the doorway looks down the whole
hall between them. Above 1.82 m that wall is pierced by arched windows 2.75 m wide on a 4.5 m
pitch, open to the dark outside, and one of them is over the doorway: a 4.4 m cabin panel
covers a window wherever it goes on this wall, and the opaque cabin is what hides the hole.

**Overgrown.** A 20 x 20 m maze of yellow wallpaper under a plaster ceiling at 2.43, the floor
one moss quad at model y = 0, the outer walls single quads on x, z = +-10.01 facing inward, two
shut rusted doors on the z = +-10 walls at x about -7 with a red-emissive EXIT sign over each.
Everything green is `BLEND` foliage cards, alpha-tested by the loader at 0.5 (crisp edges,
depth-correct, no sorting); the bushes ship with no `metallicFactor`, which glTF reads as a
fully metallic material and which drew them as dark metal, so the level's `Load` names
`Bush_*`, `Grass_*` and the moss in `metallic_override` and they render as the dielectrics
they are. The wallpaper shares the moss's normal map at a normal `scale` of 0 -- which the
loader now honours, baking the scale into the packed normal xy; before that every wall was
grained with moss. The whole room is one floor and all of it is reachable. The elevator spot is
on the south wall (z = +10.01), the one stretch of outer wall 4.5 m wide with open floor in
front of it that is not a door -- from a maze wall meeting the outer wall at x = -3.7 to the end
of the central block at x = 1.9, the cabin centred at x = -0.9 -- and the arrival looks -z into
the maze with the central block's wall 2.4 m ahead and corridors either side.

Screenshots, for the record of the look: `--scene 17` and `--scene 18` at the four yaws, and
`--pos 0.9,25,-8.5 --pitch -89` over the Overgrown room, whose ceiling is single-sided and
shows the whole maze from above (the pool's roof is not; its plan was a throwaway occupancy
raster, not shipped -- the tests re-measure every number the level is placed by).

## The window

`ext/window.rs`: a small framed window hangs on the Backrooms' hall wall between the second
and third portraits, and through it is the Overgrown room -- green and dim, behind glass. It
is locked. It is also a prop: pick it up (E), and it rides the crosshair like the cube or the
teapot, laid flat against whatever the crosshair hits and resized by perspective; walk back
from a wall with it and it grows. Unlocked by its key, stood on a wall and grown to a door,
it is one.

**How it works.** The far side has to exist for the ported renderer to draw it: the level
loads the Overgrown room a second time, two kilometres east of the hall (`window::FAR2`,
a kilometre past the Backrooms' own far world), from `level18.rs`'s own `load_spec` and
`placement` shifted by that much, with its own fence, ground cap and fall-out respawn
(`interior::build`, the objects-only half of `interior::load`: no word about the player or
the scene's mood, and the whole of it stood somewhere else). The copy is the level to the
bit: the elevator's doorway is cut out of its wall exactly as the level cuts it, because the
glTF cache keys on the whole `Load`, cut boxes included, and the same cut means one parse
shared by the copy and the level -- which is what makes the crossing below cost nothing
(`[load] scene 18 in 2 ms`). A cut wants a cabin in it, and a second `Elevator` in the scene
would overwrite the hall's own ambient channels from its update, so the cabin is built where
the level builds its own, for the same cut, moved to the copy and drawn from a wrapper that
never updates it (`window::Still`). The copy is past the mood split, so it grades as the
interior it is; the meadow, the sea and the title's backdrop never reach it.

The window itself is a `Physical` with one hit sphere, which is how the grab recognises a
prop, and its opening is a `Portal` in the scene's portal vector, with a partner in the copy
on the west wall of the corridor that runs south along it (`window::PARTNER`, measured from
the file by a test: a quarter of a metre off the wall so the arriving head sphere is never
inside it, a maze wall 1.5 m to one side, the room open 5.8 m ahead). **Every fixed step the
window re-places and re-connects the pair**: the opening at the frame's outer face, at the
frame's yaw and the opening's half-size times the frame's `p_scale`, the partner at the same
scale and at the same height as the frame -- so whatever height the window is hung at, an eye
that goes in so far above the frame's centre comes out the same height above the partner's
-- and then `portal::connect`, which bakes both transforms into the warp; a window carried or
rescaled since the last connect would otherwise warp to a stale place. Two matrix products,
500 times a second.

The frame is a box, not a flat frame, and the opening is its outer face, `window::DEPTH`
(4.5 cm at unit scale) out from the wall: the player's head is a sphere of 0.2 round the eye
and the collision pass keeps it out of the wall, so an opening flush with the wall could
never be walked through. The depth scales with the window, and the size gate is what makes a
passable window deep enough -- 28.5 cm at the smallest passable scale. Four `cube.obj` bars
in `Textures/window_frame.bmp` (painted wood from `tools/gen_window.py`, an unlit look
through the `cutout` shader, since the bake around it is unlit too), every bar with its
length along the cube's y so the texture's darkened edge runs along the bar and nothing is
stretched along it.

**Lock, size and pose.** `window::Opening` is the state, a pure function of three things,
in this order: locked -> `Locked`, the opening is `Portal::tint`ed greenish glass
(`LOCKED_TINT`), `Portal::passable` is off and a collider-only rectangle over the opening
(`Mesh::colliders_only`, `Collider::rect`) stops the player leaning into it; the frame lying
on a floor or a ceiling -> `Flat`, the portal is parked 200 m below the frame rather than
tilted (portals must stay vertical: `try_portal` re-aims only the yaw, `Portal::draw`
asserts it) and the frame is a picture frame lying there; under `PASS_HEIGHT` (1.9 m) tall
-> `Small`, clear but shut; else `Open`, a door. The HUD line while the crosshair is on it
says which: `LOCKED - IT NEEDS A KEY`, `STAND IT UP ON A WALL`, `TOO SMALL - GRAB IT AND
STEP BACK`, nothing when open. The key is a sibling's; it raises
`room::request_unlock_window` and the window takes the flag on its next step.

`ObjectT::engine_collision` is false for the window: its hit sphere is centred on the wall
and the collision pass would push it out of the wall every step, and the portal pass must
never warp it through its own opening; nor is its body ever integrated. So nothing of the
engine moves a placed window, on a wall, a floor or a ceiling -- the grab reads the hit
sphere and writes its position itself -- while it still blocks through its rectangle and is
still picked, carried and resized.

**One way.** Through the opening is the copy, and a `RoomLogic` in `level16.rs` loads the
real Overgrown level the moment the player is east of `window::CROSSING_X`, with their
position brought back by `FAR2` and their look (yaw and pitch, read off the camera transform
the room logic is handed) left for the level through `window::set_arrival` /
`window::take_arrival`; `Level18::load` stands them there instead of at its elevator. The
copy and the level being the same model at the same placement, nothing is seen to change --
no fade. There is no window on the far side; the elevator brings you back.

**Dev flags.** Two hidden ones, with `--scene`: `--window-scale S` builds the window at
physical scale `S` and `--unlock-window` builds it as if the key had been used. They set
`window::set_preset` for the process, so a restart gives the same window again.

```sh
# The small locked window: green-tinted grass through it, the hand cursor and the hint
daydreams --windowed --mute --scene 16 --pos 987,1.5,0.5 --yaw 180 --frames 60 --shot w1.bmp
# Unlocked and grown to a door (2.1 m): the room through it, clear
daydreams --windowed --mute --scene 16 --unlock-window --window-scale 7 --pos 987,1.5,0.5 --yaw 180 --frames 60 --shot w2.bmp
# Walk through: expect `[load] scene 18`, the shot from inside the Overgrown room, the
# `[shot]` position in that level's coordinates (walked on from x = -8.86, z = -9.3)
daydreams --windowed --mute --scene 16 --unlock-window --window-scale 7 --pos 987,1.5,1.0 --yaw 180 --forward --frames 240 --shot w3.bmp
# Locked, or unlocked but small: no load, the walk stops at the pane (z = 1.82)
daydreams --windowed --mute --scene 16 --pos 987,1.5,1.0 --yaw 180 --forward --frames 240 --shot w4.bmp
daydreams --windowed --mute --scene 16 --unlock-window --pos 987,1.5,1.0 --yaw 180 --forward --frames 240 --shot w5.bmp
# Carried: E on the first frame picks it up, the strafe walks it along the wall
daydreams --windowed --mute --scene 16 --unlock-window --pos 987,1.5,-0.5 --yaw 180 --ride-at 1 --strafe --frames 100 --shot w6.bmp
```

The window hangs with its centre at 1.35 m rather than eye height: grown to a door it must
still fit under the far room's 2.43 m ceiling, and the partner hangs at the frame's height.
The Backrooms' load grows by the copy, about 90 ms (`[load] scene 16 in 255 ms`, from 165);
the crossing itself, with the model shared, is the 2 ms above. The rules -- the state from
lock, pose and size; what is passable, tinted and said; the portal's transform from the
frame; the partner's match and the warp's rigidity; the parked portal; the arrival channel
-- are unit tested with detached portals, and `level16.rs` measures the window's wall and
the partner's from the two files.

## glTF loader

`ext/gltf_model.rs` is the one path every non-OBJ model takes: the door, the Backrooms scan,
and the three Sketchfab assets the later levels are built from -- EFX's animated elevator
(`Meshes/elevator_with_animation_lowpoly.glb`) and Blenderust's overgrown room and flooded
tiled complex (`backrooms_room_with_plants_overgrown.glb`, `level_37_flooded_tiled_complex.glb`,
all CC-BY-4.0; see `THIRD_PARTY.md`). What it accepts and what it does with it:

| | |
|---|---|
| **Formats** | GLB only, images embedded as PNG or JPEG; a URI image is a load error naming it, not a missing map. `KHR_materials_unlit`, `KHR_materials_emissive_strength` and `KHR_texture_transform` are honoured -- the last by baking offset, rotation and scale into the primitive's UVs at load, so the shaders never see it. Only the **base colour** texture's transform is read, and it is applied to every map of that material (a primitive has one UV stream here, and `gltf` 1.4 exposes no transform for the normal map anyway). A file without tangents gets them computed from its UVs, as the spec asks. Clearcoat, specular and the second UV set are ignored. A primitive with no material gets the spec's default material, not the file's first. |
| **Materials** | PBR maps are packed into three RGBA textures per material -- base colour + alpha, normal xy + roughness + metalness, emissive + occlusion -- each at the size of the largest map feeding it (1x1 for a factor-only material); factors, emissive strength, occlusion strength and the normal map's `scale` are baked in. Unlit materials go up as shipped. A part is drawn with one shader, so it is all PBR or all unlit. One gotcha is the spec's: a material that writes no `metallicFactor` is fully **metallic** (the default is 1.0), and a metal has no diffuse, so a foliage card exported with nothing but a base colour map renders as a dark cut-out. `Load::metallic_override` names such materials (`"Bush_*"`, a trailing star for a prefix) and the metalness they should have had; a pattern matching nothing is an error. |
| **Alpha** | A policy, not the file's word. `OPAQUE` is opaque. `MASK` is alpha-tested at the file's cutoff; `BLEND` is alpha-tested at 0.5 too -- foliage cards want crisp, depth-correct edges, not sorting artefacts -- *unless* the material is named in `Load::translucent` (`"Water.002"`), when it is drawn in a second pass after every opaque pass of the model: blended `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`, depth-tested, not depth-writing, both faces. A name the file lacks is an error. |
| **Interior lighting** | `Shaders/gltfpbr.frag` under `mood > 1.5` (`view::MOOD_INTERIOR`) drops the sun for a hemisphere -- warm white from above, a dim brown bounce from the floor -- plus a small ambient, a damped overhead specular, the same hemisphere as the metals' environment, and the material's emission, saturating (no HDR target). The Backrooms' return door takes it through the meadow split; a scene that is an interior from the first step calls `view::set_scene_mood(MOOD_INTERIOR)`, which answers for every eye and is cleared on the next load. |
| **Cuts** | `Load::cut_boxes` names axis-aligned boxes in the model's space to carve out as the file is parsed (`ext/carve.rs`): triangles inside a box are dropped, triangles crossing its faces are clipped so exactly the part outside survives, with UV, normal and tangent interpolated along the cut and the double-sided copy rebuilt from the cut front faces. `GltfModel::triangles` reports the cut geometry, so a collider built from it has the same hole the drawing has. This is how the elevator's cabin gets a real doorway through a host wall; a part wholly inside a box is an error. |
| **Animation** | Translation channels only (`LINEAR`, `STEP`; `CUBICSPLINE` reduced to its keys), by node name: `GltfModel::animation(name)`, `Animation::translation(node, t)` in the node's local space, clamped to the clip (a NaN time reads as the rest pose), and `GltfModel::node_delta(anim, node, t)` -- the node's displacement from rest in the model's fitted space, parent chain and fit scale applied. `Animation::duration` is the last key over the kept channels. A moving node is gathered as its own part with `PartSpec { roots: &["Door1"], frame: Frame::Scene, .. }`, left out of the body with `skip`, and drawn at the elevator's `Object` plus the delta. `ext/gltf_prop.rs` does exactly that for any `Load`: every part at one `Object`, triangle collision from the solid materials only (no foliage cards, no water), every part's opaque pass before any part's translucent one, a clip playing on the parts that follow a node. |
| **Looking at a file** | `daydreams --windowed --view-glb PATH [--view-translucent Water.002] --shot out.bmp` opens a scene of that one model at its own scale under the interior light, on a dark ground cap over an invisible floor, the player at the floor of its bounding box, the first clip looping; `--pos`, `--yaw` and `--pitch` work as with `--scene`. `cargo run --release --example glb_probe -- PATH` prints what the file holds first: images, primitives and materials, alpha modes, texture transforms, the node tree and the clips with their channels. |

Every new piece of the loader is unit-tested without a GL context: the UV transform, channel
sampling and clamping, `node_delta` through a parent chain, the generated tangents, the image
decode of the elevator's JPEGs and PNGs, a one-triangle GLB with no materials at all (and one
whose only material is `BLEND`, which has nothing solid and builds no collider), the cut on a
quad straddling a box and on the elevator's floor, and a `parse()` of each of the three
shipped GLBs checking triangle counts, bounds, the elevator's clip and its door deltas, the
foliage's alpha test, the moss and marble tiling, the water, the solid subset a collider
takes, and the metalness override.

### Dev tooling

```bash
cargo run --release -- --scene 14 --shot out.bmp --frames 120 --yaw 30 --pitch -5
```

skips the title, loads scene 14, aims the camera, renders 120 frames (so physics settles), writes a
screenshot and exits. This is how the shaders were iterated without a human in the loop, and how
the throughput numbers above were measured (600 frames, wall-clock).

The command line is `clap` (`src/app/cli.rs`); `--help` lists it in full, and a value that does
not parse is an error rather than a silent default (`--frames ten` used to run 90 frames;
`--scene 99` names the registry's range). What `daydreams --help` prints:

```
DayDreams

Usage: daydreams [OPTIONS] [COMMAND]

Commands:
  gen-terrain  Regenerate Meshes/meadow_tile.obj from ext::terrain::height and exit

Options:
      --windowed           Open a 1280x720 window instead of taking the whole display
      --no-vsync           Swap interval 0, so `[shot]` frame times measure the renderer rather than the panel
      --mute               No sound for this run, whatever the saved setting says (also: DAYDREAMS_MUTE=1). The `M` key cannot lift it and nothing is written to the settings file
      --assets <DIR>       Directory holding Shaders/, Meshes/, Textures/ and assets/ (also: DAYDREAMS_ASSETS)
      --log-level <LEVEL>  off, error, warn, info, debug or trace (also: DAYDREAMS_LOG). Default info
      --no-log-file        Log to the terminal only; do not write the per-user log file
      --scene <N>          Skip the title and load scene N (0-based, in key order)
      --shot <FILE>        Save a screenshot here after --frames frames and quit. Alone: the title screen
      --frames <K>         Frames to render before the screenshot, so physics settles [default: 90]
      --yaw <DEG>          Camera yaw in degrees (with --scene). Default 0
      --pitch <DEG>        Camera pitch in degrees (with --scene). Default 0
      --pos <X,Y,Z>        Player position (with --scene)
      --forward            Hold W for the whole run
      --strafe             Hold A for the whole run
      --sprint             Hold Shift for the whole run
  -h, --help               Print help
  -V, --version            Print version
```

`--shot` **without** `--scene` leaves the menu alone and photographs whatever the game boots into,
which is the title screen and `scenes::INTRO` (the Backrooms) running behind it — loading a scene would close the
menu that is the thing being looked at. `--yaw`, `--pitch`, `--pos` and the held keys only mean
something with `--scene`.

`--pos x,y,z` places the player; `--windowed` opens a 1280×720 window instead of taking the
display; `--mute` (or `DAYDREAMS_MUTE=1`) silences the run without touching the saved setting, which is what every automated screenshot or benchmark should pass; `--no-vsync` requests a swap interval of 0 so the `[shot]` line's second half — `avg
frame X ms, p95 Y ms over N frames`, measured over the frames after the first ten — reports what
the renderer costs rather than what the panel allows. `--forward` / `--strafe` hold `W` / `A` down for
the whole run and `--sprint` holds `Shift`, so the `[shot]` position print shows how far the
player walked — or, with `--forward --sprint`, ran — in the frames before the shot, and the
shot itself shows the sprint's 68° projection. `--strafe --sprint` covers the same ground as
`--strafe` alone: a sidestep never sprints. `[load] scene N in M ms` is printed on every
scene load. The numbers in [Load time and frame cost](#load-time-and-frame-cost) are
`--shot --frames 600 --no-vsync` at fullscreen.

The `[shot]`, `[load]` and `[grass]` measurement lines are **info**-level log lines (see
[Logging](#logging)); the tooling that greps for them needs the default level or higher, and
`--log-level warn` hides them. `--shot` paths are taken as given, relative to the working
directory like any other output file. Finder's `-psn_0_NNN` argument is filtered out before
parsing, so a double-clicked bundle starts clean.

`daydreams gen-terrain` is the one subcommand: it rewrites `Meshes/meadow_tile.obj` under the
asset root from `ext::terrain::height` and exits (see [Meadow](#meadow-grass-and-clouds-scene-)).

Nine flags are hidden from `--help` because they are tools rather than features: `--panic-test`
(the crash dialog, below), `--view-glb PATH` with `--view-translucent NAMES` (a scene of one
model, see [glTF loader](#gltf-loader)), `--arrive` and `--ride-at FRAME` (with `--scene`:
load it as an elevator ride would, and press E once on that frame; see
[Elevator](#elevator)), `--window-scale S` and `--unlock-window` (with `--scene`: how the
Backrooms' window is built; see [The window](#the-window)), `--drop-props H` (with
`--scene`: lift the rigid-body props by `H` metres at the start; see
[Real physics](#real-physics--extphysicsrs-extrigidrs)), and `--hold-key` and `--e-at FRAME`
(with `--scene`: start with the painting's key in hand, and press E on that frame as the
keyboard would -- the press slot, seen by the frame's first fixed step and latched for the
grab; see
[The key in the painting](#the-key-in-the-painting--extkeyrs-extpaintingrs-shaderspaintingfrag)).
`--view-glb` excludes `--scene`; its path is taken under the working directory when a file is
there, under the asset root otherwise.

The key's runs, from the Backrooms (scene 16):

```bash
# From the sweet spot, looking at the canvas centre: the painted key reads as a key on the
# first frame, TAKE THE KEY shows, and by frame 60 the real key floats in front of the canvas
cargo run --release -- --windowed --mute --scene 16 --pos 994.4,1.5,1.55 --yaw -100.9 --pitch 2.2 --shot out.bmp --frames 1
cargo run --release -- --windowed --mute --scene 16 --pos 994.4,1.5,1.55 --yaw -100.9 --pitch 2.2 --shot out.bmp --frames 60
# Square on to the portrait: the smear
cargo run --release -- --windowed --mute --scene 16 --pos 997,1.5,0.3 --yaw 180 --pitch 3 --shot out.bmp --frames 60
# A metre off the spot: nothing emerges
cargo run --release -- --windowed --mute --scene 16 --pos 993.4,1.5,1.55 --yaw -100.9 --pitch 2.2 --shot out.bmp --frames 60
# The use: key in hand, aimed at the north wall, E on frame 30. Expect
# "[key] used on the window: unlock requested", then the key gone by frame 60. Without a
# window in the scene a stand-in lock is planted a metre ahead, and says so.
cargo run --release -- --windowed --mute --scene 16 --hold-key --pos 987,1.5,1.2 --yaw 180 --e-at 30 --shot out.bmp --frames 60
```

### Logging

Every line the game has to say goes through the `log` facade (`src/app/logging.rs`) to two
sinks: the terminal, as `[LEVEL] message`, and a file with timestamps and module paths in the
per-user log directory -- `~/Library/Application Support/DayDreams/logs/` on macOS,
`%LOCALAPPDATA%\DayDreams\data\logs\` on Windows, `$XDG_DATA_HOME/daydreams/logs/` on Linux. One
file per session, named `daydreams-YYYYMMDD-HHMMSS-PID.log` (UTC in the name, local time
inside), the newest five kept. The level is `--log-level`, else `DAYDREAMS_LOG`, else `info`;
both sinks run at the same level. `--no-log-file` leaves the file out, for CI and for parallel
screenshot jobs. The file is written unbuffered, one record per write, so the last lines before
a crash are on disk when the dialog opens.

Levels, by what they mean here: **error** for an asset or GL failure and a panic; **warn** for
a subsystem running degraded (no audio device, vsync refused, no log directory, an incomplete
offscreen framebuffer); **info** for the startup banner, the asset root, the `[shot]` / `[load]`
/ `[grass]` measurements and gamepad connects; **debug** for gameplay chatter (`[grab]`,
`[cube]`, mute toggles). The mp3 decoder's own narration is filtered out of both sinks.

### Crash dialog

The game starts fullscreen with the cursor locked, so before this a panic was a screen that
went black and a desktop that came back, with the message on a stdout nobody launched from
Finder could see. `src/app/crash.rs` installs a panic hook at the top of `main` that logs the
message, location and a force-captured backtrace to the log file, shows a native error dialog
(`rfd`) with the message and the log file's path, and then hands over to the default hook so
the process still unwinds the way winit expects (`panic = "unwind"`). The same dialog is what
`app::crash::fatal` shows for an asset that cannot be loaded, followed by exit code 1.

The dialog is shown from the main thread only: off it, rfd would block the panicking thread
while dispatching to main, and if main is at that moment joining that very thread (the glTF
decoder's scoped threads are), neither side could proceed -- the scope's re-raise reaches the
hook on main a moment later and shows the dialog then.

Before it blocks on the dialog the hook hands the display back -- cursor ungrabbed and shown,
fullscreen left -- through a `Weak<Window>` that `main.rs` registers once the window exists;
otherwise a fullscreen panic put the alert behind a black borderless window with the pointer
locked.

**Headless runs never get the dialog.** `--shot` and `gen-terrain` imply it, and
`DAYDREAMS_NO_DIALOG=1` in the environment is the same switch for any other run that has
nobody to press OK; the log line is identical either way, plus an info line saying the dialog
was skipped. This matters more than a hang: on macOS the alert is drawn by a system daemon on
the process's behalf, not by the process, so it **survives a kill** -- a script that times out
and kills the game leaves the alert on the desktop until someone dismisses it by hand.

A hidden `--panic-test` flag panics after the first frame, from inside a winit callback with
the window up, which is where a real one would come from; it was used to confirm that the
dialog appears over the game on macOS without deadlocking and that the log carries the
backtrace. Set `DAYDREAMS_NO_DIALOG=1` when scripting it.

### Typed asset errors and the fatal sink

`src/app/error.rs` is `AssetError` (`thiserror`): `Io { path, source }`, `ShaderCompile { path,
log }`, `ShaderLink { name, log }`, `BadBmp { path, reason }`, `Gltf { path, reason }`,
`Gl(String)` and `NoAssetRoot { tried }`. `Shader::new`, `Texture::new`, `Mesh::new`,
`FrameBuffer::new` and `GltfModel::load` return it where they used to `panic!`, `expect` or --
for a missing mesh -- silently draw nothing. `Resources::acquire_*` and `GltfModel::acquire`
keep their infallible signatures, because the hundred-odd scene call sites have no `Result`
to carry an error through, and hand any `Err` to `app::crash::fatal`.

Two pieces of the ported loaders became pure functions on the way, the same liberty `parse_obj`
took in `mesh.rs`: `shader::scrape_attribs` (the `"\nin "` scan that assigns attribute slots,
including its malformed-declaration error) and `texture::decode_bmp` (the header and pixel walk
for 24-bit atlases and 32-bit BGRA, with a truncated file as an error instead of an
out-of-bounds slice). Both are unit-tested against tiny in-memory inputs and the shipped files.

### Asset root

`src/app/assets.rs` resolves the directory holding `Shaders/`, `Meshes/`, `Textures/` and
`assets/` once, before the window exists, and every loader joins onto it (`assets::path`). The
order, first directory containing `Shaders/` wins:

1. `--assets DIR` or `DAYDREAMS_ASSETS` -- and an explicit choice that does not qualify is an
   error, not a fall-through;
2. `../Resources` relative to the executable when it lives in `*.app/Contents/MacOS/`;
3. the executable's own directory;
4. the current directory;
5. the crate root baked in at build time (`CARGO_MANIFEST_DIR`), so `cargo run` works from
   anywhere on the machine that built it.

Nothing qualifying is `AssetError::NoAssetRoot` listing every directory tried, through the
fatal sink: `DAYDREAMS_ASSETS=/nonexistent daydreams` shows the dialog, logs the list and
exits with code 1. `--shot` output paths and `settings.toml` are not assets and are not
resolved here.

### Scene registry

`src/ext/scenes.rs` is the one table a scene is declared in: `SceneEntry { key, name, make }`,
in key order -- CodeParade's seven first, in the registration order of `Engine.cpp:41-47`, then
`8` `9` `0` `-` `=` `[` `]` `\` `;` `'` `,` `.`. `Engine` builds its scene vector from it, the
level-select menu reads the names from it and the key loop walks it; `scenes::INTRO` is the
index NEW GAME and the title backdrop use, resolved from the name `"Backrooms"` by
`scenes::index_of` -- a `const fn`, so reordering the table cannot start the game somewhere
else, and the same lookup the elevator turns its floor names into indices with at runtime. To
add a scene, append an entry and make sure `input::key_index` maps its key -- the registry's
tests check that there are nineteen entries, that keys and names are unique, that
`SCENES[INTRO]` is the Backrooms, that `index_of` finds every name and nothing else, that every
constructor builds, and that every key byte is reachable from a physical `KeyCode`.

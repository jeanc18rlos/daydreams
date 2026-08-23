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

There are **218** such comments. Every one of them is summarised under
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
cargo test --release   # 165 tests: Matrix4/Vector3 algebra, the .obj parser against the shipped meshes, and the extensions' pure logic
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

`Cargo.toml`'s `[lints.clippy]` allows, with a reason on each, the lints that would ask for
idiomatic rewrites of transcribed C++ — `approx_constant` for the original's `GH_PI` literal,
`manual_strip` for `Mesh.cpp`'s line parser, `needless_range_loop`, `assign_op_pattern` over the
`Vector.h` types, and so on — plus `assertions_on_constants` for the level tests that deliberately
check tuned layout constants against each other. Everything else is on. `rustfmt.toml` sets the
100-column, small-heuristics-off style the source was written in; `clippy.toml` lifts
`too_many_arguments` to nine for `ui.rs`'s `draw_quad`.

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

**check** is red until the formatting and clippy passes land -- `fmt --check` still reports
hunks across most of the source, and clippy is clean under `-D warnings` only once the quality
pass that follows this one lands; the other jobs are green.

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
- `std::shared_ptr<T>` → `Option<Rc<T>>`, a null `shared_ptr` being `None` (`object.rs:33`).
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

`cargo build --release` — 0 errors, 0 warnings.
`cargo test` — 9 passed, 0 failed.

---

# Extensions beyond the port

Everything above documents the faithful port. Everything below is **new work** — it has no C++
counterpart and is not part of HackerPoet/NonEuclidean.

The split is enforced by convention and visible in the source: the port carries **220
`// PORT:` comments** citing the original line each deviation came from, while additions carry
**`// EXT:`** comments. New code lives in `src/ext/` and `src/level7..16.rs`; the ported files
were touched only where a hook was unavoidable, and each of those is a handful of lines.

## New scenes

| Key | Scene | Idea |
|----|-------|------|
| `8` | **Perspective Gallery** | Forced-perspective grabbing. Pick something up and where you release it decides how big it really is. |
| `9` | **Penrose Ascent** | Four descending corridors wired into a closed cycle. Walk forward to fall forever, turn around to climb forever. |
| `0` | **Compound** | Carry a grabbed object through a scaling portal so both size effects multiply. Neither source game does this. |
| `-` | **Unobserved** | Statues that only move while you are not looking at them, across two portal-linked chambers. |
| `=` | **Anamorphic Chamber** | Twelve scattered fragments that resolve into a ring from exactly one spot in the room. |
| `'` | **Backrooms** | The intro's meadow and door, but the door opens onto a scanned, light-baked office maze with real wall, floor and furniture collision. See [Backrooms](#backrooms-scene-) below. |

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

### Per-frame room logic — `ext/room.rs`

The ported `Scene` trait has exactly one method, `Load` (`Scene.h:7-9`) — scenes build objects
and then have no further say. Rather than change that trait, rooms needing behaviour push a
`RoomLogic` object: an ordinary `ObjectT` with no mesh and no shader, so `Object::Draw` skips it
(`Object.cpp:21` only draws when both are present) while `Engine::Update` still ticks it.

The one thing such logic cannot reach is the player — `Load` receives `&mut Player`, never the
`Rc` the engine keeps it in — so a room that needs to move them (the Backrooms, when they have
fallen under its floor) calls `request_respawn`, and the engine applies it at the end of the
same step, after the portal pass, through `set_position`: `prev_pos` moves with `pos`, so the
next step's `try_portal` sees no segment that could sweep a doorway.

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

**Sound effects** — drop files into `assets/sfx/` named `grab`, `release`, `portal`, `land` or
`footstep`. `grab`, `release` and `footstep` have call sites; `portal` and `land` are loadable
but nothing fires them yet. None of the files ship.

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
  warm caches, and the old objects drop afterwards. The portals are the exception and are
  dropped first: there is nothing in one worth keeping warm -- the eager framebuffers each
  used to own (some 60 MB a portal, which keeping twelve of them across the load of twelve
  more would have doubled) live on the engine now, shared per recursion level -- and the mesh
  and two shaders a portal re-acquires are pinned in `ExtState`, which is what keeps the
  reload at the `< 1 ms` above.
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
| `collider.rs` | read-only `mat()` accessor, so rays can transform the rectangle to world space |
| `input.rs` | four analog fields, filling the `//Joystick //TODO:` slot; `E`/`M`/`R` and the scene keys `8`–`'` (`8` `9` `0` `-` `=` `[` `]` `\` `;` `'`); `Shift` into the `VK_SHIFT` slot and the resolved sprint multipliers |
| `player.rs` | stick axes added to the keyboard move and look vectors; sprint multipliers on the speed cap, acceleration and bob rate, and a footfall counter |
| `object.rs` | `UpdateCtx` carries the player's eye transform, so room logic can see where you look; `RenderCtx` carries the pass frustum, eye and the shared portal framebuffers, and `draw_impl` culls by bounding sphere; `ObjectT::trimesh()` for triangle-mesh scenery |
| `frame_buffer.rs` | sized attachments instead of `GH_FBO_SIZE` square |
| `engine.rs` | one `ext` field, the scene vector, names and keys read from the [registry](#scene-registry), a grab tick, the sprint resolve, scene-load notification; the portal frustum pre-test and the one-frame-late occlusion slots; the old scene's objects kept alive across `load_scene` (its portals dropped first); the triangle-mesh rounds in the collision pass; a room's respawn request applied after the portal pass; the `--forward`/`--strafe`/`--sprint` held keys |
| `portal.rs` | the nested pass scissored to the quad's screen footprint |
| `shader.rs` | memoised by-name uniform lookup (misses cached too), `set_mat4`; `new` returns `Result<_, AssetError>` and the attribute scan is a pure, tested `scrape_attribs` |
| `texture.rs` | `new` returns `Result<_, AssetError>`; the BMP byte walk is a pure, tested `decode_bmp` |
| `mesh.rs` | `new` returns `Result<_, AssetError>`: a missing `.obj` is an error, not an empty mesh |
| `resources.rs` | every `acquire_*` turns a loader's `Err` into `app::crash::fatal` |
| `props.rs` | `Sky::draw` takes the eye from the inverse it already computes, and draws at the far plane under `GL_LEQUAL` so it can go last |
| `main.rs` | gamepad polling in `about_to_wait`; the platform layer's startup order (panic hook, command line, logging, asset root -- all in `src/app/`); the hidden `--panic-test`; key levels dropped on focus loss |

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
geometry but pass through each other. They cannot be stacked.

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

The intro again -- same meadow, same white door, both built by `ext/meadow.rs`, which the two
scenes share along with the far world's origin and the title screen's vantage -- except that
through the door is
`Meshes/backrooms_vr.glb`: a Sketchfab light-bake of the Backrooms, 29 primitives, 70k
triangles, 27 maps, every material `KHR_materials_unlit`. Three things had to exist for it to be
a place rather than a picture:

| What | Where |
|------|-------|
| **A general glTF load.** The door loader fitted one model to one height and packed PBR maps at 512. `Load { fit, max_map }` now chooses between that and `Fit::Identity` (source metres, source origin -- the scan is already to scale and already Y-up once the root node's rotation is applied), and per-material `unlit` skips the PBR pack entirely: the base map goes up as shipped, at its own size and with its own wrap mode, and `baseColorFactor` / `emissiveFactor x KHR_materials_emissive_strength` are uniforms. A part is drawn with one shader, so its materials must all be unlit or all PBR -- the loader asserts it. The door's path is byte-identical. | `ext/gltf_model.rs`, `Shaders/gltfunlit.*` |
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

Frame cost, measured with `glFinish` after each frame on a shared M3 Max at 2560x1440 (so
absolute numbers are pessimistic; the comparison is what matters): meadow spawn in the intro
**8.1 ms**, the same spawn in the Backrooms (the portal now draws the hall) **8.6 ms**, standing
in the hall looking down it **6.1 ms**, looking back at the door (the portal draws the meadow)
**8.0 ms**. Physics at 500 Hz with the triangle collider is inside those numbers.

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
      --assets <DIR>       Directory holding Shaders/, Meshes/, Textures/ and assets/ (also: DAYDREAMS_ASSETS)
      --log-level <LEVEL>  off, error, warn, info, debug or trace (also: DAYDREAMS_LOG). Default info
      --no-log-file        Log to the terminal only; do not write the per-user log file
      --scene <N>          Skip the title and load scene N (0-based, in key order)
      --shot <FILE>        Save a screenshot here after --frames frames and quit. Alone: the title screen
      --frames <K>         Frames to render before the screenshot, so physics settles [default: 90]
      --yaw <DEG>          Camera yaw in degrees (with --scene) [default: 0]
      --pitch <DEG>        Camera pitch in degrees (with --scene) [default: 0]
      --pos <X,Y,Z>        Player position (with --scene)
      --forward            Hold W for the whole run
      --strafe             Hold A for the whole run
      --sprint             Hold Shift for the whole run
  -h, --help               Print help
  -V, --version            Print version
```

`--shot` **without** `--scene` leaves the menu alone and photographs whatever the game boots into,
which is the title screen and the intro level running behind it — loading a scene would close the
menu that is the thing being looked at. `--yaw`, `--pitch`, `--pos` and the held keys only mean
something with `--scene`.

`--pos x,y,z` places the player; `--windowed` opens a 1280×720 window instead of taking the
display; `--no-vsync` requests a swap interval of 0 so the `[shot]` line's second half — `avg
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
`8` `9` `0` `-` `=` `[` `]` `\` `;` `'`. `Engine` builds its scene vector from it, the
level-select menu reads the names from it and the key loop walks it; `scenes::INTRO` is the
index NEW GAME and the title backdrop use. To add a scene, append an entry and make sure
`input::key_index` maps its key -- the registry's tests check that there are seventeen entries,
that keys and names are unique, that `SCENES[INTRO]` is the intro, that every constructor
builds, and that every key byte is reachable from a physical `KeyCode`.

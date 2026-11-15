# rustudio on Android

The Android app is the same editor as the desktop one — the `rustudio`
library (src/lib.rs) — inside a `NativeActivity`. What differs is in
`src/android/` (compiled only for `target_os = "android"`) and in
`crates/rustudio-android/` (the `cdylib` the activity loads, with the
`manifest.yaml` xbuild turns into the AndroidManifest). Targets:
`aarch64-linux-android` (arm64-v8a, every phone and tablet) and
`x86_64-linux-android` (the emulator).

## Building

CI (`.github/workflows/android.yml`) builds four APKs on every push / PR
that touches Rust sources — arm64 and x64, debug and release — and uploads
them as artifacts (`rustudio-<arch>-<profile>.apk`). A `v*` tag builds the
two release APKs and adds them to the tag's GitHub release beside the
desktop downloads (`rustudio-android-arm64-v8a.apk`,
`rustudio-android-x86_64.apk`, `SHA256SUMS-android`); the editor's update
notice (src/update.rs) stays off on Android, where the system installs
APKs. Locally:

```sh
# once
rustup target add aarch64-linux-android x86_64-linux-android
sudo apt install clang lld llvm            # the NDK targets link with these
cargo install --git https://github.com/rust-mobile/xbuild \
  --rev e67b501cbe0e9ca5436c223aa7ec0fe5e27544d1 xbuild
x doctor                                   # what it found; nothing here fails

# an APK (xbuild fetches its NDK sysroot and android.jar into ~/.cache/x)
x build -p rustudio-android --platform android --arch arm64 --format apk
x build -p rustudio-android --platform android --arch arm64 --format apk --release
ls target/x/{debug,release}/android/rustudio-android.apk

# on a device / emulator
adb install -r target/x/debug/android/rustudio-android.apk
adb logcat -s rustudio RustStdoutStderr
```

**Install the release APK to judge speed.** The debug one builds the
editor at opt-level 1 with debug assertions and overflow checks in every
crate (Cargo's dev profile): several times slower in the physics, the Luau
runtime and the renderer's CPU side, and a phone shows it first. The
start line in logcat and the first `perf:` line say which one is running
(`debug build, aarch64 android` / `release build, …`).

`x build` without `--pem` signs with xbuild's debug key (installs with
adb). A release key goes in as `--pem key.pem` or the `X_PEM` environment
variable; CI reads it from the `ANDROID_SIGNING_PEM` secret when set.

Without a device, `cargo check` for the target works with the same
environment xbuild uses (sysroot, `--target=aarch64-linux-android33`, clang
and lld) — see the `Build APK` step of the workflow for the variables.

xbuild quirks worth knowing (commit e67b501):

- A workspace member list must not contain `"."`: xbuild walks
  `workspace.members` and rejects a member whose manifest has a
  `[workspace]` table (*Did not expect a `[workspace]`*). Cargo adds the
  root package by itself, so the root isn't listed.
- `opengles_version` in `manifest.yaml` can't be used: it is written as
  `android:glEsVersion="0x00030000"` but xbuild's own manifest compiler
  parses integer attributes as decimal and `x build` stops at
  *[3/3] Create apk — invalid digit found in string*. The GLES fallback
  doesn't depend on the declaration (wgpu picks it at run time), and every
  API 26 device has GLES 3.0, so the manifest declares Vulkan only.

## What is where

| Concern | Desktop | Android |
|---|---|---|
| Entry point | `src/main.rs` → `rustudio::desktop_main()` | `crates/rustudio-android/src/lib.rs`: `rustudio_on_create` (the manifest's `android.app.func_name`) → android-activity's `ANativeActivity_onCreate` + the destroy hook; `android_main` → `rustudio::android::run()` |
| Window / GPU | eframe + winit (Wayland, X11, Windows, macOS) | eframe + winit on `NativeActivity` (`android-native-activity`); wgpu Vulkan, GLES 3 fallback (`wgpu_options` in src/lib.rs) |
| Lifecycle | — | eframe's wgpu backend drops the window + surface in `suspended()` and recreates them in `resumed()`; a rotation is a resize and reconfigures the swapchain |
| Folders (`src/paths.rs`) | `~/.local/share`, `~/.config`, `~/.local/state`, `~/.cache`, `~/Pictures` | `getExternalFilesDir()` for data (Studio content, places), `getFilesDir()/config`, `/state`, `getCacheDir()`, `<external>/Pictures` (`src/android/mod.rs`); `HOME` and the `XDG_*` variables are set to the same so `dirs` (rustudio-account) agrees |
| `api-dump.json` | next to the executable / app bundle / source tree | APK `assets/`, unpacked to `files/resources/` at first start (`src/android/assets.rs`) |
| File dialogs | rfd (async, on a worker thread) | rustudio's own picker (`src/android/storage.rs`) over the app folder and, once the storage permission is granted from the picker's own button, shared storage (Download, Documents, …); "Open with rustudio" from file managers through the VIEW intent (`content:` URIs copied in via ContentResolver) |
| Input | mouse + keyboard | fingers through `src/touch_input.rs` (below); hardware mice / keyboards work as on a desktop |
| Clipboard | egui-winit (arboard) | egui's copies mirrored to `ClipboardManager` after each frame, pastes read from it (`src/android/clipboard.rs`) |
| Browser links (`platform::open_url`) | `ctx.open_url` (webbrowser) | `ACTION_VIEW` Intent from the activity (`jni::open_url`); eframe's `links` feature isn't built for Android |
| Back button | — | closes what is open on top (Escape), otherwise `moveTaskToBack` — the activity isn't destroyed (`src/android/lifecycle.rs`) |
| Activity destroyed | — | `onDestroy` hook: close request, then `process::exit(0)`; `android_main` always leaves the process when the loop ends (`lifecycle.rs`) |
| UI zoom | 1.0 of the display's scale | first run: the zoom that fits ~1100 points across (0.8 … 1.6 of the density), then View > Zoom In / Out as saved |
| Graphics quality | level 10 (Studio's defaults) | Automatic from level 3: the 3D view at 0.7 scale (0.5 while the camera moves, scaled up by EASU + RCAS) with 512² textures, 900 studs of view distance, the lite shader, 2× anisotropy, no MSAA, one shadow cascade, Play capped at 30 fps (View > Graphics Settings, the 1–10 bar; "Graphics quality" below); saved with the layout |
| Logging | stderr | logcat tag `rustudio` (`log` → android_logger), stdout/stderr forwarded by android-activity |
| Crash reports | `~/.cache/rustudio/crash/` | `<cache>/rustudio/crash/` (same `src/crash.rs`: panic hook + signal handlers chain to debuggerd) |
| Studio content download | ureq + rustls (ring) | the same; started only when ConnectivityManager reports a validated network |
| Login keyring | `keyring` 4 (Secret Service / Keychain / Credential Manager) | `keyring-core` + `android-native-keyring-store` (SharedPreferences + Android Keystore) behind the same `Entry` API (`crates/rustudio-account/src/store.rs`) |
| TLS roots (`rustudio-account` MessagingService socket) | OS store (`rustls-native-certs`) | Mozilla's (`webpki-roots`, enabled alongside): Android's CA store isn't in a form `rustls-native-certs` reads; `ureq` already used `webpki-roots` |

## Audit: desktop-only code

What the Android build compiles out or replaces, and why. Everything not
listed compiles unchanged for the Android targets (checked with `cargo
check --target aarch64-linux-android`).

Gated with `#[cfg(not(target_os = "android"))]` (would not compile):

| Item | Reason | Android replacement |
|---|---|---|
| `rfd` (Cargo.toml) and the rfd code in `src/file_dialog.rs` | rfd 0.15 has no Android backend | `file_dialog::Request` + `android::storage::Picker` |
| `libc::getloadavg` in `src/platform.rs::load_average` | Bionic has it only from API 29; the symbol is absent from the libc crate's Android bindings | returns "?" |
| `eframe` features `wayland`, `x11` | desktop window systems | `android-native-activity` (target-specific dependency tables in Cargo.toml) |

Desktop-only by behaviour (compile fine, never active on Android, left
ungated so the shared code paths stay one):

- `src/hypr.rs` (Hyprland IPC through `hyprctl`), `src/layout_store.rs`
  window geometry — no `HYPRLAND_INSTANCE_SIGNATURE`, nothing happens.
- `src/mouse_lock.rs` — pointer lock / confine logic over egui types; no
  pointer on a touch screen, so the Default/LockCenter paths are no-ops.
- `src/play.rs` gamepad scan of `/dev/input/by-id` — the folder isn't
  readable in an app sandbox; gamepads will come through winit / `AInputEvent`
  in a later step.
- `src/app.rs` and `src/selftest/fidelity.rs` self-relaunch with
  `current_exe()` (`--screenshot`, `--self-test`) — harness-only, never
  reached without a command line (`Harness::interactive()`).
- `src/lib.rs` `rerun_headless` / `restore_parent_fds` — already
  `cfg(target_os = "linux")` only.
- `src/macos.rs`, `windows-sys` — already cfg'd by OS.
- `crates/rustudio-account` `keyring` 4 — not built for Android (no store
  there: every `Entry::new` failed, which broke the session check and the
  Quick Sign-in flow on the first device runs). `store.rs` has a
  `keyring` shim module over `keyring-core` with
  `android-native-keyring-store` as the default store, so `session.rs` and
  `store.rs` compile unchanged.
- `src/domains/media_video.rs` `dlopen` of libvpx / libopus — Android has
  no system copies; the loaders fail gracefully (no video decoding).

## Audit: Roblox services on a device

Column "today" is what the shared Luau/Rust layer already does (the same
on every system); "Android" is what the device plumbing in `src/android/`
provides or still has to.

| Service / member | Today (shared) | Android plumbing |
|---|---|---|
| `UserInputService.TouchEnabled / MouseEnabled / KeyboardEnabled / AccelerometerEnabled / GyroscopeEnabled / PreferredInput` | constants in `runtime_ext.rs device_prop()` (desktop values) | **to do**: true/false from the activity (touch screen, `sensors::available()`), `PreferredInput = Touch`, `PreferredInputChanged` |
| `TouchStarted / TouchMoved / TouchEnded`, `TouchTap`, `TouchTapInWorld`, `TouchLongPress`, `TouchPan`, `TouchPinch`, `TouchRotate`, `TouchSwipe` | declared (`runtime_ext.rs` 116-154), never fired; `__rs_input_event` carries mouse/keyboard | **to do**: winit `Touch` events (egui `PointerButton` + `multi_touch`) → `InputObject` with `UserInputType.Touch` per finger id, gesture recognisers, `gameProcessedEvent` from egui's `wants_pointer_input` |
| `GetDeviceAcceleration / GetDeviceGravity / GetDeviceRotation`, `DeviceAccelerationChanged / DeviceGravityChanged / DeviceRotationChanged` | zeros (`gui_input.luau` 299-307, `library.luau` 852) | `sensors.rs` (ASensorManager: linear acceleration, gravity, gyroscope, game rotation vector, 60 Hz on its own thread) — **wiring to Luau to do** (g units, CFrame from the quaternion) |
| `UserInputService.ModalEnabled` | stored | **to do**: hides the default thumbstick / jump button once those exist |
| `OnScreenKeyboardVisible / OnScreenKeyboardSize / OnScreenKeyboardPosition`, `TextBoxFocused / TextBoxFocusReleased` | focus events fire (`play_game.rs` 247); keyboard size zero | `jni::window_insets().ime_bottom` (API 30+), `AndroidApp::show_soft_input / hide_soft_input` on TextBox focus — **wiring to do** |
| `HapticService.IsVibrationSupported / IsMotorSupported / SetMotor / GetMotor` | gamepad-only flags, in-memory motor table (`gui_input.luau` 1796-1845) | `jni::vibrator_info()` (hasVibrator, hasAmplitudeControl) and `jni::vibrate(ms, strength)` — **wiring to do**: `UserInputType.Touch` supported, `VibrationMotor.Large` (and `Small` when amplitude control exists) |
| `HapticEffect:Play / Stop`, `HapticEffect.Type` presets | simulated durations (UIHover .035 s, UIClick .055, UINotification .4, GameplayCollision .3, GameplayExplosion .8) | `jni::vibrate_pattern` (VibrationEffect.createWaveform) — **mapping of each preset to do** |
| `ContextActionService.BindAction(..., createTouchButton)`, `GetButton / SetTitle / SetImage / SetPosition` | bindings stored, priority dispatch (`library.luau` 671-767); buttons never drawn | **to do**: draw the buttons in the play viewport (`ContextActionGui` ScreenGui as Roblox does: 70×70 ImageButton, default positions right of the thumbstick), route taps to the bound function with `UserInputType.Touch` |
| `GuiService:GetGuiInset()`, `TopbarInset`, `GetInsetArea`, `ScreenGui.ScreenInsets` (`None / DeviceSafeInsets / CoreUISafeInsets / TopbarSafeInsets`), `ScreenGui.SafeAreaCompatibility` | `(0, 58)` constant (`mech::GUI_TOP_INSET`), `TopbarInset` zero rect, inset root in `gui_input.rs` ~498 | `jni::window_insets()` (system bars + display cutout; the 4 sides) — **wiring to do**: DeviceSafeInsets = those, CoreUISafeInsets = those + topbar, TopbarSafeInsets = topbar only |
| `StarterGui.ScreenOrientation`, `PlayerGui.ScreenOrientation / CurrentScreenOrientation` | stored (`start_page/builtin.rs` 144); `CurrentScreenOrientation = LandscapeLeft` | `jni::set_requested_orientation` + `jni::current_orientation()` — **wiring to do**: apply at Play start, restore `sensorLandscape` at stop |
| `Player:GetMouse()`, `Mouse` while playing | desktop pointer | **to do**: the first touch stands in (as Roblox does) |
| `VRService`, gamepads | stubs | unchanged |

## Fingers (`src/touch_input.rs`)

egui-winit makes the first finger the mouse pointer with the left button
down, which is wrong for most of the editor: a finger laid on an Explorer
row starts dragging the row instead of scrolling the list, and a finger on
the 3D view box-selects. `Touch::filter` runs in `App::raw_input_hook`,
before egui sees the frame's events, and rewrites the `Event::Touch`
stream (dropping egui-winit's stand-in pointer events):

| Finger | Becomes |
|---|---|
| moves at once (> 8 pt before 320 ms) on the chrome | a scroll of the panel under it (`MouseWheel` events in a Start/Move/End phase, applied 1:1), with a fling after the lift |
| still for 320 ms on the 3D view | a left press (grabs the part or handle under it), dragging from then on |
| still for 320 ms on the chrome, then moves | a left press where it landed, dragging from then on (Explorer rows, text selection, sliders, splitters, windows) |
| still for 650 ms on the chrome | a right click where it landed: the context menus (Explorer rows, dock tabs) and the text fields' Cut / Copy / Paste / Select all menu (`src/android/text_menu.rs`); no left press goes out before it, so a selection under the finger is kept for the menu; the lift is nothing (and if no menu opened, a move from there is the drag below after all) |
| lifted without moving | a tap (press + release at the landing point) |
| moves at once on the 3D view | a right-button drag: the editor camera's look; in Play the game camera (the PlayerModule's right-drag orbit, mouse deltas from the finger) |
| on a transform handle | a left press at once (drags the handle) |
| second finger on the 3D view | ends the first one's drag; `InputState::multi_touch` gives the viewport a pinch (zoom toward the fingers; in Play the game's wheel: `mech::PINCH_NOTCHES_PER_DOUBLING`) and a two-finger pan |
| in Play, lands bottom-left / bottom-right of the view | the movement stick (W/A/S/D held by its deflection, drawn under the finger) / the jump button (Space) — `touch_input::play_zones`; released when Play stops or the window loses focus. A quick tap in the stick zone is still a tap (GUI buttons there keep working). Stick and jump fingers are kept out of egui's touch stream, so a look finger beside them is never read as a pinch |

The viewport publishes where things are each frame (`Zones`: its rect,
whether the game has the input, the stick and jump areas, a handle under
the pointer) and draws the stick and button in Play (`Touch::paint`). The
chrome's egui settings for fingers are in `android::touch::setup`. The
tuning numbers (hold 320 ms, long press 650 ms, slop 8 pt, fling, stick
and button sizes) are `mech::TOUCH_*` with their rows in
docs/MECHANICS.md ("Touch screens") and mechanics.json. The module has no
Android dependency, so its twelve tests run on the desktop
(`cargo test touch_input`).

### Text: the soft keyboard, Cut / Copy / Paste

winit never resizes an Android window for the soft keyboard (the
manifest's `adjustResize` moves NativeActivity's content rect; the surface
egui draws on stays the whole screen), so `src/android/keyboard.rs` makes
the room: while a text field has the focus, or the room is still made, it
asks the Java main thread for the keyboard's height every 100 ms
(`jni::keyboard_height_later`: `WindowInsets.getInsets(Type.ime())` on
API 30+, the system inset less the stable inset before that; the editor's
thread never waits for the answer, which comes a frame or two later) and
takes that much off the root `Ui` at the start of `App::ui` — the dock,
the 3D view and the status bar move up as an Android view would — and
gives it to egui as the bottom safe-area inset, so windows, menus and
popups keep above it too (set every frame, zero when the keyboard is
down: egui keeps the last inset it was given). When the room is made or
changes, the focused field's caret is scrolled into view
(`keyboard::reveal_cursor`, from the Properties cells and the script
editor: the field's `TextEditOutput` gives the caret's row).

The keyboard is shown and hidden by egui-winit alone, from egui's IME
output (up while a text field has the focus: `set_ime_allowed` →
android-activity's `show_soft_input` / `hide_soft_input`); the user
hiding it with Back or its own hide key is invisible to the app. Two
things made that a loop on the first device runs. The Properties and
Explorer edit fields asked egui for the focus every frame while an edit
was open, and egui counts every `request_focus` — a field's own on a tap
inside it too — as an IME interruption (`should_interrupt_composition`),
which egui-winit answers with a hide and a show: the fields now ask only
while they lack the focus and are not just back from losing it
(`properties.rs`, `explorer.rs`, `account_ui.rs`). And every height
reading was acted on at once: the zero read during a hide-and-show took
the room away (a re-layout of every panel), the show put it back, and
releasing the focus on a short zero re-showed the keyboard the next tap
asked for. The readings now go through hysteresis (`Keyboard::follow`):
the first height above zero is taken at once (`soft keyboard up: N px, M
ms after the focus` in logcat), a different height only after two equal
readings running, a zero while a field is focused is ignored for 600 ms —
longer than a hide-and-show's two animations and than the hide and show
of a focus move between fields — and only then releases the field's focus
(`soft keyboard hidden by the user`): the edit ends with its text kept, as
a click away ends it, and a tap on the field starts it again. A zero with
nothing focused takes the room away at once.

The soft keyboard has no Ctrl, egui's text fields have no menu of their
own and egui-winit has no clipboard on Android, so a long press on a text
field opens `src/android/text_menu.rs`: Cut, Copy, Paste (the system
clipboard's text, `jni::clipboard_get`) and Select all, fed to egui as
the events the shortcuts would be (`Event::Cut` / `Copy` / `Paste` /
Ctrl+A) through `raw_input_hook` on the next frame; the copies reach
`ClipboardManager` through `src/android/clipboard.rs`. The menu keeps the
field's focus for its tap (egui's rule is a click outside takes it) and
puts back the selection egui's right click collapsed. Objects are cut,
copied and pasted from the Explorer's long-press menu as before.

The layer is **off on the Start Page** (`Touch::set_enabled` from
`raw_input_hook`, while `start_page_ui` would draw): that page is plain
egui — lists that scroll by drag (`ScrollArea`'s own touch rule), cards
that are taps, long presses for its menus — and has none of the editor's
reasons to tell a scroll from a drag. Switching off lets go of anything
held; a finger down at a switch stays egui's until it lifts, so no press
is left half delivered. Once a place opens, the layer is on.

## The first Play on a phone: Studio's content

Roblox's default character (the R15 body with its face and dynamic head),
its `Animate` and `Health` scripts, and the PlayerModule whose
CameraModule follows the character and whose ControlModule moves it all
come from Studio's own content, which rustudio has none of in the APK: at
its first start the editor downloads it from Roblox's CDN (about 60 MB
compressed, 13 packages plus a hundred or so retired textures fetched one
by one; `src/studio_content.rs`) into the app's files folder
(`…/files/rustudio/studio`). That takes a few minutes on a phone. Until
it is there, Play spawns rustudio's stand-in character — plain blocks, no
face, no animation — and no camera or movement script follows it; Output
says so at the spawn (`Studio's content isn't installed yet…`), lists
each package as it comes (`Studio content: downloading …zip (3/13)…`)
and ends with `Studio content <version> installed in …` or an error. Stop
and press Play again after that line (fonts need a restart). The download
goes on where it stopped at the next start when the app is killed
meanwhile (packages already unpacked are kept in `.studio-partial`), and
retries a dropped connection a couple of times by itself. Uninstalling
the app removes the content with its data; `adb install -r` keeps it.

## Frame time on a phone

Every few seconds the editor logs a line like

```
perf: 136 frames in 5.0 s: 36.9 ms apart (p95 72.3, worst 89); editor 5.2 ms + egui 1.2 ms on the CPU, 30.5 ms GPU/vsync wait; 3D GPU 9.8 ms per drawn frame = shadows 3.1 + scene 4.2 + ssao/bloom/dof 1.5 + post 1.0 (68 measured); window 1550x720 px at 1.40 px/pt, 3D view 816x310 px (scale 0.70, AO off, shadow maps x0.50), 3D frames drawn 68 / shown again 68; sections (ms/frame): dock_panels 4.3, viewport 3.4, vp_render 3.0
```

(`src/perf_log.rs`; `adb logcat -s rustudio`, or `RUSTUDIO_PERF_LOG=1` on
a desktop). "editor" is `App::ui`'s own time, "egui" the layout and
painting eframe adds, the "wait" what is left of the interval (the GPU,
vsync, the loop). "3D GPU" is the renderer's own GPU time by timestamp
queries (`render::GpuTimer`, where the device has them), per frame drawn
afresh and by section; what the wait holds beyond it is egui's own GPU
pass and vsync. The first line names the GPU and the wgpu backend (Vulkan
is the one wanted; a GL fallback is slow). An idle editor logs nothing: no
input, no frames.

Switches for a device, which has no shell environment to launch from:

```
adb shell setprop debug.rustudio.env "RUSTUDIO_SUN_SHADOWS=0 RUSTUDIO_VIEW_REUSE=0"
```

then relaunch (`setprop debug.rustudio.env ""` clears). Space-separated
`RUSTUDIO_*=value` pairs, 91 characters at most (`android::export_env`;
the renderer's are read by `quality::RenderSettings::with_env`):
`RUSTUDIO_QUALITY=auto|1..10` (`low` / `medium` / `high` = 3 / 6 / 10)
and the single settings over the level — `RUSTUDIO_VIEW_DISTANCE`
(studs, 0 unlimited), `RUSTUDIO_DETAIL_PX` (the smallest part drawn, in
pixels), `RUSTUDIO_TEXTURE_TEXELS` (256, 512, 1024), `RUSTUDIO_MSAA`
(1, 2, 4), `RUSTUDIO_HDR` (0|1), `RUSTUDIO_RENDER_SCALE` (0.25 to 1),
`RUSTUDIO_SUN_SHADOWS` (0: the sun's shadow maps off, as
Lighting.GlobalShadows false), `RUSTUDIO_SUN_CASCADES` (0 to 4),
`RUSTUDIO_SHADOW_SCALE` (the shadow maps' texels across, 0.25 to 1; see
below), `RUSTUDIO_LIGHT_SHADOWS` (0|1), `RUSTUDIO_SSAO` (0|1),
`RUSTUDIO_BLOOM` (levels, 0 off), `RUSTUDIO_DOF` (0|1), `RUSTUDIO_CLOUDS`
(0|1), `RUSTUDIO_NO_ENV_PROBE`, `RUSTUDIO_MOTION_SCALE` (the render scale
while the camera moves or a game runs, 0.25 to 1), `RUSTUDIO_UPSCALE` (0:
egui's bilinear blit in place of EASU + RCAS), `RUSTUDIO_ANISOTROPY` (the
texture samplers' taps: 1, 2, 4, 8, 16), `RUSTUDIO_LITE` (the lite
shader's bits, 0 to 31; below), `RUSTUDIO_FPS_CAP` (Play's frames per
second at most, 0 none) — and `RUSTUDIO_VIEW_REUSE` (0|1),
`RUSTUDIO_FRAME_LATENCY` (swapchain images less one, 1 to 3; see below),
`RUSTUDIO_NO_PIPELINE_CACHE`, `RUSTUDIO_NO_PIPELINE_WARMUP` (below). A
switch wins over the saved Graphics Settings choice for the run.

### Graphics quality (`src/quality.rs`, `src/graphics_settings.rs`)

The 3D view's per-frame fixed costs on the device — the 4× MSAA
Rgba16Float scene and its resolve, a full-resolution Rgba16Float copy of
it for glass and water, the five-level bloom pyramid, the half-resolution
SSAO chain, two full- and eleven quarter-resolution depth-of-field
targets, the sun's four cascades, the local lights' shadow atlas and the
cloud layer — were paid whatever the scene held: an empty baseplate took
31–37 ms of GPU a frame at 0.7 scale; and the scene's own costs — every
part in the frustum drawn however far and small, every material at 1024²
— grew with the place. View > Graphics Settings has Studio's bar, a level
from 1 (fastest) to 10 (Studio's defaults; the tables are `QUALITY_*` in
`src/mech.rs`), or Automatic, and each setting apart from the level
("Auto" follows it); View > Graphics Quality has the same levels as a
menu. A phone starts in Automatic at level 3, a computer at 10
(`Graphics::platform_default`); the choices are kept in the saved layout
and the level mirrors Studio Settings > Rendering > Editor Quality Level
(`settings().Rendering.EditQualityLevel`: Automatic or Level01–21, two
Studio levels per bar step), set from either side.

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|
| View distance (studs) | 500 | 700 | 900 | 1200 | 1500 | 2500 | 4000 | 6000 | 10000 | all |
| Smallest part drawn (px) | 6 | 4 | 3 | 2 | 1.5 | 1.2 | 1 | 1 | 1 | 1 |
| Material textures | 256² | 256² | 512² | 512² | 512² | 1024² | 1024² | 1024² | 1024² | 1024² |
| MSAA | off | off | off | off | off | 2× | 2× | 2× | 4× | 4× |
| Scene format | RG11B10F | RG11B10F | RG11B10F | RG11B10F | RGBA16F | RGBA16F | RGBA16F | RGBA16F | RGBA16F | RGBA16F |
| Render scale | 0.5 | 0.6 | 0.7 | 0.8 | 1 | 1 | 1 | 1 | 1 | 1 |
| Scale while the camera moves | 0.5 | 0.5 | 0.5 | 0.6 | 0.7 | 0.8 | 1 | 1 | 1 | 1 |
| Texture filtering (anisotropic taps) | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 4 | 8 | 8 |
| Lite shader (bits, below) | 7 | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Play frame-rate cap | 30 | 30 | 30 | — | — | — | — | — | — | — |
| Sun shadow cascades | 0 (voxel sun) | 1 | 1 | 2 | 2 | 2 | 3 | 4 | 4 | 4 |
| Shadow texel scale | 0.5 | 0.5 | 0.5 | 0.5 | 0.75 | 0.75 | 1 | 1 | 1 | 1 |
| Local light shadows | off | off | off | off | on | on | on | on | on | on |
| SSAO | off | off | off | off | off | off | off | on | on | on |
| Bloom levels | 0 | 0 | 0 | 3 | 3 | 3 | 5 | 5 | 5 | 5 |
| Depth of field | off | off | off | off | off | off | off | on | on | on |
| Clouds, environment probe | off | off | off | off | on | on | on | on | on | on |

(Rg11b10Ufloat where the device renders to it, else Rgba16Float. Roblox's
own levels for the view distance: about 500 studs at Level01, 100000 at
Level21; the rest of the table is an estimate until the next device run
measures it.)

**Automatic** (`quality::Auto`; Studio's QualityLevel.Automatic) watches
the frames the 3D view draws back to back (frames shown again from the
last one and frames after an idle pause don't count): the median frame
over the last 2 s longer than 1.35 × a 60 Hz refresh → a level down; 20 s
of frames at the refresh (within 1.1 ×) → a level up, but not back into a
level left in the last 90 s; the half second after a change, pipelines
and targets being remade, isn't counted. Each step logs `graphics
quality (automatic): level N: …` with the settings in effect.

The individual settings (the window's rows; `quality::Overrides`): Render
Distance (500 to 10,000 studs or Unlimited), Part Detail (Low / Medium /
High: parts under 4 / 2 / 1 px skipped), Texture Quality (256² / 512² /
1024²), Texture Filtering (trilinear, or 2× to 16× anisotropic), Shading
(Simple: the lite shader, bits 7; Full), Anti-aliasing (off, 2×, 4×), HDR
Scene (32- or 64-bit), Render Scale (50–100 %), Scale While Moving
(40–100 %, never above the render scale), Play Frame Rate (uncapped, 30,
45, 60), Sun Shadows (Off / Low: one cascade at half texels / Medium: two
at 0.75 / High: Studio's four), Light Shadows, Ambient Occlusion, Bloom,
Depth of Field, Clouds, Reflections (the scene environment probe). "All
Auto" clears them, "Restore Defaults" the platform's start.

What the renderer does with the settings (`render.rs`):

- **View distance and part detail** (`Renderer::render`): a static batch
  cell, a moving part or a terrain chunk wholly farther than the view
  distance from the camera is not drawn, and a part whose bounding radius
  covers fewer pixels than the detail threshold (`mech::LOD_MIN_PX` at
  level 7 and up) is skipped — what a 2–3× density phone screen never
  showed at a stud or two a pixel. The sun's casters keep the cascades'
  own reach.
- **Texture quality** (`MaterialArrays`, `remake_materials`): the three
  material arrays (albedo, surface, emission; 12 MB a 1024² layer with
  mips, a quarter of that per halving) are made at the setting's size and
  take each pack's mip chain from the level of that size; a change remakes
  them from the packs kept in memory. The terrain shader's "never finer
  than Studio's 512²" clamp reads the array's size (`textureDimensions`)
  and stands down for smaller arrays.

- **The scale while the camera moves, and the frame scaled up**
  (`viewport::Motion`, `RenderSettings::scale_now`, `Pipe::Easu`,
  `Pipe::Rcas`, `Target::upscale`): a frame is drawn at the motion scale
  while the camera's view matrix changed within the last 0.25 s
  (`mech::QUALITY_MOTION_HOLD_S`) or a game runs, at the render scale
  otherwise, and a camera come to rest gets its frame at the render scale
  once the hold is over (the viewport asks for that repaint). What the
  pixels don't show while things move, they don't have to be drawn — the
  phone's 0.5 scale is a quarter of the fragments. A frame drawn under
  the panel's size is brought up to it by AMD's FidelityFX Super
  Resolution 1.0 written in WGSL (`shader.wgsl` "upscaling": EASU, the
  edge-adaptive 12-tap kernel, then RCAS, the contrast-adaptive sharpening
  at `mech::UPSCALE_SHARPNESS`), two full-screen passes over the
  tone-mapped image into panel-sized textures, in place of the bilinear
  blit egui's image did (`RUSTUDIO_UPSCALE=0` brings that back); egui
  shows RCAS's output.
- **The lite shader** (`RenderSettings::lite`, shader constant `LITE`;
  levels 1 to 3 build with bits 7, `RUSTUDIO_LITE=0..31` chooses): the
  part and terrain fragment shaders compiled without what a phone screen
  at 0.5 scale doesn't show — bit 1 no normal maps (parts' material and
  SurfaceAppearance normals, the terrain's normal blend; the geometric
  normal stands), bit 2 reflections from the ambient cube instead of the
  environment probe (`env_reflect`; the probe isn't drawn below level 5
  either way), bit 4 a single read per material (no detiling's second,
  rotated read, no Wang tiling's four), bit 8 no material maps at all
  (the part's colour, roughness and metalness from the material's
  constants), bit 16 no studs (the legacy surface atlas). Every varying
  the fragment shader doesn't interpolate (`PartOut`'s per-instance
  values) is `@interpolate(flat)` in every build. The lite bits are part
  of the build key: a change rebuilds the pipelines.
- **Texture filtering** (`RenderSettings::anisotropy`, `repeat_sampler`,
  `WaterPass::new`): the part textures', material maps' and water
  normals' samplers take the level's anisotropic taps — 2 to level 7
  (Studio's own default is `mech::TEXTURE_ANISOTROPY` = 8), 8 at the
  top; a phone GPU's anisotropic filtering costs a tap per sample per
  degree, and at 0.5 scale no one tells. A change remakes the samplers and
  the part textures' bind groups (the key holds it).
- **Play's frame-rate cap** (`RustudioApp::cap_frame_rate`,
  `RenderSettings::fps_cap`; 30 at levels 1 to 3, `RUSTUDIO_FPS_CAP`):
  while a game runs, a frame that comes early waits out its period before
  eframe presents it — fewer frames drawn, less heat, longer battery; the
  editor isn't capped (it repaints on input). The wait is kept out of
  Automatic quality's frame time (`Editor::quality_tick`): a capped frame
  isn't a slow one.
- **Compressed material textures** (not done): the material arrays are
  RGBA8 (12 MB a 1024² layer; 3 MB at 512²); ASTC 4×4 would be a quarter
  of that and half the bandwidth, and every Vulkan phone GPU has it (the
  perf log's first lines say `texture compression: ASTC, ETC2, no BC` for
  the device). The packs are decoded from Studio's PNG/DDS at load, so
  compressing them needs an ASTC encoder on the device (a minute of CPU
  for 30 packs at 1024²) or a cached, pre-encoded pack set; neither is in
  this port. The 256² and 512² levels and the skipped mips cut the memory
  the same way for now.
- **Pipelines and targets only for what is on** (`Pipe::wanted`,
  `Target`): the SSAO, bloom, depth-of-field, glass, water, cloud and
  light-shadow targets exist, and their passes run, only at a level that
  uses them; the sun's atlas holds the level's cascades (shader
  constant `SUN_CASCADES` hands the rest to the voxel sun); without MSAA
  the scene is drawn straight into the resolved texture (`MSAA_SAMPLES`
  1; `render::msaa_wgsl` turns the shaders' multisampled depth reads
  into plain ones).
- **Pipelines compiled when first needed** (`Pipes`, `Renderer::pipe`),
  the first frame's set in the background right after the shader module
  (`warm_pipelines`; `RUSTUDIO_NO_PIPELINE_WARMUP=1` leaves it all to
  first use), and Vulkan's pipeline cache kept across runs in the app's
  cache directory (`DiskCache`, keyed by adapter and driver;
  `RUSTUDIO_NO_PIPELINE_CACHE=1`): the ~10 s black screen between
  adapter selection and the first frame was one shader module and some
  forty pipelines compiled in a row before anything was drawn.
- **A settings change rebuilds only what the key covers**
  (`RenderSettings::build_key`: MSAA, format, shadow scale, light
  shadows, the lite bits, the anisotropy → `Renderer::rebuild`); the rest
  — passes on or off, the render and motion scales, the cascade count,
  the frame-rate cap — applies to the next frame.
- The perf line (every 5 s) names the level and the settings in effect
  and how many pipelines are built of those wanted.

First measurements (a Galaxy Note20 Ultra, Adreno 650 on Vulkan, release
APK, the window 1550 × 720 px at 60 Hz, the 3D view 816 × 310): 5–14 ms
of CPU a frame and 30–50 ms of "wait" with the camera still as with it
moving, 20–27 fps; a frame that drew the 3D view afresh cost about 10 ms
more than one that showed the last one again. The "3D GPU" figure above
is what the next run has to show (it is an illustration until then).

What the port does about it:

- **Three swapchain images** (`lib::frame_latency`; `RUSTUDIO_FRAME_LATENCY`
  1 to 3 overrides): egui's low-latency default asks for one frame of
  latency, two images. Android's compositor keeps the image on screen
  until it has latched the next one, so with two images every frame first
  waits for that — a vsync of nothing before any drawing, half the
  refresh rate at best, which is the ~31 ms "wait" above with the camera
  still (egui notes the same halving on iOS and leaves the default there).
  With a third image the next frame starts while the last one is still
  queued, and the CPU's work overlaps the GPU's.
- **Level 3 on a first run, in Automatic** (the table above): the 3D
  view at 0.7 of the panel's pixels, 512² textures, 900 studs of view
  distance, no MSAA, one shadow cascade, the passes an empty baseplate
  paid for off — and a level down after two seconds of slow frames.
- **Half-size sun shadow maps** (`sun_shadow::texel_scale`; the level's
  `shadow_scale`, `RUSTUDIO_SHADOW_SCALE` overrides): Studio's four
  cascades are 384 / 768 / 1024 / 2048 texels
  across in a 3200 × 2048 atlas, and a cascade that moved with the camera
  is drawn into the depth map and then blurred twice (EVSM) over its whole
  tile — the far cascade alone is 2048² × 8 bytes read five times and
  written twice, which a phone GPU's bandwidth pays for in frames rather
  than fractions of one. On Android the tiles, atlas and depth map are
  half as many texels across (a quarter of the fill and blur); the blur
  keeps its width in studs (`Cascade::level` counts texels), and a cascade
  whose texels are still finer than the view's pixels looks the same.
  `cargo test sun_shadow` keeps the full-scale layout at Studio's numbers.
- **Ambient occlusion off** below level 8 (Graphics Settings > Ambient
  Occlusion; `RUSTUDIO_SSAO`): Studio's SSAO is five passes over the view
  for a darkening in corners a phone's screen barely shows.
- **The last 3D frame shown again** while nothing in it changed
  (`viewport::ViewReuse`; `RUSTUDIO_VIEW_REUSE=0|1`, on by default on
  Android only): the frame's inputs are hashed (`render::frame_key`), the
  renderer counts its own changes (`Renderer::generation`, bumped by
  every mesh, texture, item, terrain, sky and grid upload), and a frame
  is drawn afresh when either moved, when time shows (a game running,
  terrain water, Terrain.Clouds drifting, the scene environment probe's
  frames of GPU work, world GUIs and effects, the test harness) and at
  least every 500 ms. With a still camera the chrome then redraws at egui's
  own pace — taps and scrolls in the Explorer and Properties no longer
  wait for a 3D frame each. (The first device run showed exactly every
  other frame reused: the effects system hands over a frame of effects
  whether or not it holds any, and an empty one counted as "effects on a
  reused frame", which asks for a fresh one.)
- **No cursor commands in Play.** `mouse_lock::note_window` took the
  Android window for one that can't lock the cursor, so a game's
  LockCenter grab fell back to Confined and sent a cursor position every
  frame, which winit rejected with a warning logged each time.
- The release APK is built with thin LTO (`CARGO_PROFILE_RELEASE_LTO` in
  android.yml; the workspace profile is unchanged for desktops).

Still to do, with measurements from the line above: a phone tier for the
light grid's size, the sun's far cascades redrawn less often while the
camera moves (levels 7 and up), the terrain's grass and detail passes
thinned at the low levels, particle limits and a leaner default layout
for small screens (every panel open at once is most of the CPU time on a
phone). The next device run's perf lines at each level against the ones
above are what sizes the tables' next step (their numbers are estimates
until then).

## Device-run notes (first APKs)

Found on devices with the phase-1 APKs; the fixes are in the table above
and here. What a report from a device should carry: `adb logcat -s
rustudio` filtered to `soft keyboard` (each height change, the focus
release), `clipboard` (each copy put on the system clipboard, each read
and its length, or "no clip"), `text menu` (open, cut, copy, paste,
select all), `graphics quality` (each level change with the settings in
effect) and the perf lines (every 5 s).

- **Black window at first start, until leaving and coming back.** winit
  0.30 drops `MainEvent::Destroy`, so whenever the system destroyed the
  activity under a running loop (a swipe from Recents followed by a quick
  relaunch, a configuration change outside `configChanges`), the Java
  main thread sat in `onDestroy` waiting for `android_main`; the next
  launch landed in that process and drew nothing. `lifecycle.rs` hooks
  `onDestroy` and leaves the process; `android_main` exits the process
  when the loop returns; the manifest declares every configuration change
  the API 33 android.jar knows; the first three seconds request repaints
  so an early lost redraw can't leave the splash up; `App::save` keeps the
  layout at eframe's save moments (the surface going away).
  The hook gets the `ANativeActivity` from the library's own entry point
  (`rustudio_on_create`, named by `android.app.func_name` in
  manifest.yaml), not from `AndroidApp::activity_as_ptr()`, which is the
  Java `Activity`'s JNI reference: the first phase-2 APK read that as the
  struct and crashed at start (`SIGSEGV` at a small address in
  `lifecycle::hook`).
- **Heavy lag.** The 3D view rendered at the panel's full pixel size — a
  phone's 2–3× density screen is more pixels than a laptop's — plus debug
  builds. First run now renders the view at 0.7 scale (graphics level 3;
  View > Graphics Settings), and `x build --release` (or the `profiling`
  profile) is the build to judge speed by; `RUSTUDIO_RENDER_SCALE`
  overrides.
- **Clipboard.** egui-winit has no Android clipboard, and the soft
  keyboard has no Ctrl: the long-press menu on text fields
  (`android/text_menu.rs`) and the mirror in `android/clipboard.rs`.
- **Typing behind the keyboard.** The window never shrank for the soft
  keyboard; `android/keyboard.rs` (see "Text" under "Fingers").
- **The keyboard came back as soon as it was hidden** while a Properties
  value or an Explorer name was being edited: the field asked for the
  focus every frame, which egui turns into an IME hide-and-show each
  time (see "Text" under "Fingers").
- **Ten seconds of black before the first frame; 30+ ms of GPU a frame
  for an empty baseplate.** Every pipeline compiled before the first
  frame, and every pass of the desktop renderer run at full cost: the
  graphics levels and the lazy pipelines ("Frame time on a phone").
- **Browser sign-in "always fails".** Closing the account popup (a tap
  outside it, Escape, Android's Back turned into Escape) cancelled the
  waiting sign-in; it now goes on behind the closed popup, every step is
  logged (`[Info] Roblox: browser sign-in: …`, the failure as
  `[Error] Roblox: …`), and a session Roblox accepted is kept for the run
  even when the Keystore-backed store can't save it
  (`Client::sign_in_with`). On Android every Output line goes to logcat
  (`RustStdoutStderr`), not only warnings and errors: the panel is small
  on a phone and a report needs the Info lines.
- **No character in Test Play.** Run (not Play) has no character by
  design; the other reasons a spawn is skipped now say so in the Output
  (`No character: …`), and a spawn logs `Character: R15|R6 (from the …)`
  and what the body is made of (`Character body: N parts, M of them
  MeshParts`).
- **Browser sign-in never opened the code page.** Two causes: eframe's
  `links` feature (and so `ctx.open_url`) isn't in the Android feature
  set, and `keyring::Entry::new` failed on Android so the session check
  errored before the code showed. `platform::open_url` and the keyring
  shim.
- **Opening a file asked for All-files access.** NativeActivity gets no
  `onActivityResult`, so the system document picker can't hand a choice
  back; the in-app picker lists the app's own folder without any
  permission and offers a button for shared storage (All files access on
  API 30+, the runtime permission below). "Open with rustudio" from a
  Files app needs nothing.
- **Panels didn't scroll, no camera on touch.** `src/touch_input.rs`.
- **Start Page lists wouldn't scroll / taps misfired.** The finger layer
  ran there too, with no viewport to publish its zones, over a page that
  is plain egui; it is off until a place opens (see "Fingers").
- **Lag on a Baseplate with a release APK.** The chrome redrew the 3D
  view every frame it drew itself; see "Frame time on a phone".
- **The 3D view lags while the camera moves, the chrome less so.** The
  perf lines showed ~31 ms of wait a frame with the camera still (the 3D
  view shown again, not drawn), so the frame itself, not the 3D pass, was
  the floor: two swapchain images under Android's compositor (see "Frame
  time on a phone", three images now), with the shadow cascades at half
  the texels on top for the frames that do draw.

## Later steps (in order)

1. Touch for the game: `UserInputService` touch events and gestures
   from the same finger stream (`touch_input.rs` already tells fingers
   apart), `TouchEnabled` / `PreferredInput`, `ModalEnabled` hiding the
   stick and button (they are the editor's own for now, holding keys).
2. Device properties and sensors (the `device_prop()` table, the
   `Device*Changed` events from `sensors::latest()` once per new sample).
3. Haptics (`HapticService`, `HapticEffect`) through `jni::vibrate*`.
4. Insets, orientation, soft keyboard.
5. `ContextActionService` touch buttons.

Each step closes rows in docs/ENGINE-GAPS.md; the Creator Hub reference
pages are the source of truth for every member name, argument and enum.

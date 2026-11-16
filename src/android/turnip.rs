//! The Turnip (Mesa) Vulkan driver in place of the device's Adreno driver
//! on Snapdragon phones, swapped in before wgpu loads Vulkan.
//!
//! How it works. The APK's lib/arm64-v8a carries `libadrenotools.so`
//! (TouseefX's libadrenotools-native, a fork of bylaws/libadrenotools made
//! for apps with no Java of their own; tools/android/fetch-turnip.sh puts
//! it there with its hook libraries) and `libvulkan_freedreno.so` (Mesa's
//! Turnip, nihui's Android build). libadrenotools opens a private copy of
//! the system's Vulkan loader (`libvulkan.so`) in a linker namespace of its
//! own, with a hook that makes that copy load the Turnip driver instead of
//! the vendor's `vulkan.adreno.so`, then patches the system loader's
//! `vkGetInstanceProcAddr` / `vkGetDeviceProcAddr` in place (an inline
//! hook, shadowhook) so they answer from the private copy. wgpu reaches
//! Vulkan the ordinary way — ash's `dlopen("libvulkan.so")`,
//! `dlsym("vkGetInstanceProcAddr")`, and every other entry point through
//! that one — so with the patch in place every instance, device and
//! swapchain it makes is Turnip's, with the system loader's own swapchain
//! code on top (Mesa's Android builds leave window surfaces to the loader,
//! as the vendor driver does). Nothing in wgpu or eframe changes; what
//! matters is the order: [`activate`] runs in `android_main` before eframe
//! starts, i.e. before wgpu's first `Entry::load()`, while the launch
//! screen is still up. A probe through the loader afterwards says which
//! driver answers (the log, Graphics Settings > GPU driver, the perf line's
//! adapter name).
//!
//! Only on Snapdragon: Turnip drives Adreno GPUs through Qualcomm's KGSL
//! kernel driver, so the device must have `/dev/kgsl-3d0` (or name
//! Qualcomm as its SoC maker in the system properties) and an Adreno 6xx
//! or newer; every other device keeps its driver. The swap is also skipped
//! on other ABIs than arm64-v8a, below Android 12 (the fork's floor), when
//! the files aren't in the APK, when turned off (Graphics Settings > GPU
//! driver, or `RUSTUDIO_TURNIP=0`), and after a start that didn't get to
//! its first frames with Turnip (a marker file this module keeps; the
//! window offers "Try again"). `RUSTUDIO_TURNIP=1` overrides the window's
//! choice and the marker; `RUSTUDIO_TURNIP=force` also skips the device
//! checks. `RUSTUDIO_GPU_TURBO=1` asks libadrenotools for the GPU's top
//! clocks (`adrenotools_set_turbo`; thermal limits still apply);
//! `RUSTUDIO_TU_DEBUG=<flags>` hands Turnip its `TU_DEBUG` options
//! (`sysmem`, `gmem`, `nobin`, `noubwc`, `nolrz`, …).

use std::ffi::{c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use ash::vk;

/// What [`activate`] decided, for the log and Graphics Settings.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Turnip answers this process's Vulkan calls.
    pub active: bool,
    /// The device qualifies (arm64 Snapdragon on Android 12+, files in the
    /// APK): the window offers the switch only then.
    pub available: bool,
    /// What happened, or why nothing did (one line).
    pub note: String,
    /// The Vulkan device as the probe saw it after the swap ("Turnip
    /// Adreno (TM) 650", "Adreno (TM) 650"); "" when no probe ran.
    pub device: String,
    /// Its driver ("Mesa 26.3.0", or the vendor's version).
    pub driver: String,
}

static STATUS: OnceLock<Status> = OnceLock::new();

/// The outcome of this start's [`activate`] (a default before it ran).
pub fn status() -> Status {
    STATUS.get().cloned().unwrap_or_default()
}

/// The marker files, under the app's files folder.
fn dir() -> PathBuf {
    super::files_dir().join("turnip")
}

/// Graphics Settings' choice: on unless the `off` marker is there.
pub fn wanted() -> bool {
    !dir().join("off").exists()
}

/// Graphics Settings' switch (takes effect at the next start). Turning it
/// on also forgets a failed start.
pub fn set_wanted(on: bool) {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    if on {
        let _ = std::fs::remove_file(d.join("off"));
        let _ = std::fs::remove_file(d.join("failed"));
    } else {
        let _ = std::fs::write(d.join("off"), b"");
    }
}

/// A start with Turnip didn't reach its first frames, so it is off until
/// [`retry`] / [`set_wanted`].
pub fn failed_last_time() -> bool {
    dir().join("failed").exists()
}

pub fn retry() {
    let _ = std::fs::remove_file(dir().join("failed"));
}

/// The editor drew its first frames: the start with Turnip went well.
/// Called from the frame loop (app.rs); cheap after the first time.
pub fn mark_healthy() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        if STATUS.get().map_or(false, |s| s.active) {
            let d = dir();
            let _ = std::fs::remove_file(d.join("pending"));
            // A good start after a failed one (RUSTUDIO_TURNIP=1) clears it.
            let _ = std::fs::remove_file(d.join("failed"));
            log::info!("turnip: first frames drawn, the driver swap is marked good");
        }
    });
}

/// What the device is, as far as the swap cares.
struct Device {
    /// Qualcomm's GPU kernel driver node, what Turnip's KGSL backend opens.
    kgsl: bool,
    /// ro.soc.manufacturer (API 31+: "QTI" / "Qualcomm"), ro.hardware
    /// ("qcom"), ro.board.platform ("kona", "lahaina", …), for the log and
    /// as the second opinion.
    soc: String,
    hardware: String,
    platform: String,
    /// /sys/class/kgsl/kgsl-3d0/gpu_model ("Adreno650"), "" when unreadable.
    gpu: String,
}

impl Device {
    fn read() -> Self {
        let prop = super::system_property;
        Device {
            kgsl: Path::new("/dev/kgsl-3d0").exists(),
            soc: prop("ro.soc.manufacturer"),
            hardware: prop("ro.hardware"),
            platform: prop("ro.board.platform"),
            gpu: std::fs::read_to_string("/sys/class/kgsl/kgsl-3d0/gpu_model")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        }
    }

    fn qualcomm(&self) -> bool {
        let soc = self.soc.to_ascii_lowercase();
        self.kgsl || soc.contains("qualcomm") || soc == "qti" || self.hardware.to_ascii_lowercase().starts_with("qcom")
    }

    /// The Adreno generation's first digit (6 for an Adreno 650), 0 when
    /// unknown.
    fn adreno_generation(&self) -> u32 {
        let digits: String = self.gpu.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(0)
    }

    fn describe(&self) -> String {
        format!(
            "kgsl {}, soc {:?}, hardware {:?}, platform {:?}, gpu {:?}",
            if self.kgsl { "present" } else { "absent" },
            self.soc,
            self.hardware,
            self.platform,
            self.gpu
        )
    }
}

/// The folder the installer unpacked the APK's native libraries into
/// (`ApplicationInfo.nativeLibraryDir`): where this very library was
/// loaded from. A path with "!/" in it means the libraries were not
/// extracted (extractNativeLibs=false), which libadrenotools can't work
/// with.
fn native_lib_dir() -> Result<PathBuf, String> {
    // SAFETY: dladdr fills `info` for an address inside a loaded object;
    // the address of a function of this crate is one.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::dladdr(native_lib_dir as *const () as *const c_void, &mut info) };
    if ok == 0 || info.dli_fname.is_null() {
        return Err("dladdr: can't tell where librustudio_android.so was loaded from".into());
    }
    // SAFETY: dli_fname is a NUL-terminated string the loader owns.
    let path = unsafe { CStr::from_ptr(info.dli_fname) }.to_string_lossy().into_owned();
    if path.contains("!/") {
        return Err(format!("the APK's native libraries aren't extracted ({path}); libadrenotools needs them as files"));
    }
    Path::new(&path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no folder in {path}"))
}

fn dlerror() -> String {
    // SAFETY: dlerror returns NULL or a string valid until the next dl call
    // on this thread; it is copied at once.
    let p = unsafe { libc::dlerror() };
    if p.is_null() {
        "unknown error".into()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// `dlsym` on `handle`, as a function pointer of type `F`.
///
/// # Safety
/// `F` must be the symbol's actual signature.
unsafe fn symbol<F: Copy>(handle: *mut c_void, name: &CStr) -> Result<F, String> {
    assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<*mut c_void>());
    let p = libc::dlsym(handle, name.as_ptr());
    if p.is_null() {
        return Err(format!("{}: {}", name.to_string_lossy(), dlerror()));
    }
    Ok(std::mem::transmute_copy::<*mut c_void, F>(&p))
}

/// What the loader answered after the swap.
struct Probe {
    device: String,
    driver: String,
    api: u32,
    turnip: bool,
}

/// A Vulkan instance through libvulkan.so, as wgpu will make one: whose
/// driver is behind it. The instance is destroyed again; the driver it
/// loaded stays in the process, which wgpu's own instance reuses.
fn probe() -> Result<Probe, String> {
    // SAFETY: the loader's entry points through dlopen / dlsym; each call
    // below is made with valid arguments and the instance is destroyed
    // before the function returns.
    unsafe {
        let entry = ash::Entry::load().map_err(|e| format!("loading libvulkan.so: {e}"))?;
        let loader_version = entry
            .try_enumerate_instance_version()
            .map_err(|e| format!("vkEnumerateInstanceVersion: {e}"))?
            .unwrap_or(vk::API_VERSION_1_0);
        let app_name = c"rustudio";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(1)
            .api_version(if loader_version >= vk::API_VERSION_1_1 { vk::API_VERSION_1_1 } else { vk::API_VERSION_1_0 });
        let info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = entry.create_instance(&info, None).map_err(|e| format!("vkCreateInstance: {e}"))?;
        let result = (|| {
            let devices = instance.enumerate_physical_devices().map_err(|e| format!("vkEnumeratePhysicalDevices: {e}"))?;
            let Some(&pd) = devices.first() else {
                return Err("no Vulkan physical device".to_string());
            };
            let props = instance.get_physical_device_properties(pd);
            let device = props.device_name_as_c_str().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut driver_id = vk::DriverId::default();
            let mut driver = String::new();
            // VK_KHR_driver_properties is core in 1.2 (Turnip: 1.3+; the
            // Adreno driver on Android 13 phones: 1.1, which has no name
            // for itself here).
            if loader_version >= vk::API_VERSION_1_1 && props.api_version >= vk::API_VERSION_1_2 {
                let mut dp = vk::PhysicalDeviceDriverProperties::default();
                let mut p2 = vk::PhysicalDeviceProperties2::default().push_next(&mut dp);
                instance.get_physical_device_properties2(pd, &mut p2);
                driver_id = dp.driver_id;
                let name = dp.driver_name_as_c_str().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let info = dp.driver_info_as_c_str().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                driver = format!("{name} {info}").trim().to_string();
            }
            if driver.is_empty() {
                driver = format!(
                    "vendor 0x{:04x}, driver version {}.{}.{}",
                    props.vendor_id,
                    vk::api_version_major(props.driver_version),
                    vk::api_version_minor(props.driver_version),
                    vk::api_version_patch(props.driver_version)
                );
            }
            let turnip = driver_id == vk::DriverId::MESA_TURNIP || device.starts_with("Turnip");
            Ok(Probe { device, driver, api: props.api_version, turnip })
        })();
        instance.destroy_instance(None);
        result
    }
}

fn version(v: u32) -> String {
    format!("{}.{}.{}", vk::api_version_major(v), vk::api_version_minor(v), vk::api_version_patch(v))
}

/// The switch: `RUSTUDIO_TURNIP`.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Unset: on where the device qualifies, unless the window turned it
    /// off or the last start with it failed.
    Auto,
    Off,
    /// `1`: the window's choice and the failed marker are ignored.
    On,
    /// `force`: the device checks too.
    Force,
}

fn mode() -> Mode {
    match std::env::var("RUSTUDIO_TURNIP").ok().as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("0") | Some("off") | Some("false") | Some("no") => Mode::Off,
        Some("1") | Some("on") | Some("true") | Some("yes") => Mode::On,
        Some("force") => Mode::Force,
        _ => Mode::Auto,
    }
}

/// Swaps the driver in where it applies, before wgpu exists. Never fails
/// the start: anything that goes wrong is logged under `rustudio` (and
/// libadrenotools' own lines under `AdrenoToolsPatch`) and the device's
/// driver stays.
pub fn activate() {
    let started = Instant::now();
    let status = match try_activate() {
        Ok(s) => s,
        Err(s) => s,
    };
    let took = started.elapsed();
    if status.active {
        log::info!(
            "turnip: ACTIVE — {} ({}) answers Vulkan in place of the Adreno driver; {:.0} ms",
            status.device,
            status.driver,
            took.as_secs_f64() * 1e3
        );
    } else if status.device.is_empty() {
        log::info!("turnip: not used — {}", status.note);
    } else {
        log::warn!("turnip: not active — {} (Vulkan answered with {} / {}); {:.0} ms", status.note, status.device, status.driver, took.as_secs_f64() * 1e3);
    }
    let _ = STATUS.set(status);
}

fn skip(note: impl Into<String>, available: bool) -> Status {
    Status { active: false, available, note: note.into(), device: String::new(), driver: String::new() }
}

/// Ok: a status to keep (active or not); Err: one that says why nothing
/// was done.
fn try_activate() -> Result<Status, Status> {
    let mode = mode();
    if mode == Mode::Off {
        return Err(skip("RUSTUDIO_TURNIP=0", true));
    }
    if cfg!(not(target_arch = "aarch64")) {
        return Err(skip("Turnip ships for arm64-v8a only", false));
    }
    let sdk = super::AndroidApp::sdk_version();
    let dev = Device::read();
    log::info!("turnip: device: {}; Android API {sdk}", dev.describe());
    if mode != Mode::Force {
        if !dev.qualcomm() {
            return Err(skip("not a Snapdragon (no /dev/kgsl-3d0, no Qualcomm SoC in the properties): the device keeps its own driver", false));
        }
        let gen = dev.adreno_generation();
        if gen != 0 && gen < 6 {
            return Err(skip(format!("{}: Turnip supports Adreno 6xx and newer", dev.gpu), false));
        }
        if sdk < 31 {
            return Err(skip(format!("Android API {sdk}: libadrenotools-native wants Android 12 (API 31) or newer; RUSTUDIO_TURNIP=force tries anyway"), false));
        }
    }
    let lib_dir = match native_lib_dir() {
        Ok(d) => d,
        Err(e) => return Err(skip(e, false)),
    };
    let shim = lib_dir.join("libadrenotools.so");
    let driver = lib_dir.join("libvulkan_freedreno.so");
    for (what, p) in [("libadrenotools.so", &shim), ("libvulkan_freedreno.so (the Turnip driver)", &driver), ("libmain_hook.so", &lib_dir.join("libmain_hook.so"))] {
        if !p.exists() {
            return Err(skip(format!("{what} isn't in the APK ({}); tools/android/fetch-turnip.sh puts it in before the build", lib_dir.display()), false));
        }
    }
    let markers = dir();
    let _ = std::fs::create_dir_all(&markers);
    if mode == Mode::Auto {
        if !wanted() {
            return Err(skip("turned off in Graphics Settings > GPU driver", true));
        }
        if markers.join("pending").exists() {
            // The last start never got to its first frames.
            let _ = std::fs::rename(markers.join("pending"), markers.join("failed"));
            let _ = std::fs::write(markers.join("failed"), b"");
            log::warn!("turnip: the last start with Turnip didn't reach its first frames; keeping the device's driver until Graphics Settings > GPU driver > Try again (or RUSTUDIO_TURNIP=1)");
        }
        if failed_last_time() {
            return Err(skip("off since a start with Turnip didn't reach its first frames (Graphics Settings > GPU driver > Try again)", true));
        }
    }
    // From here on a crash would be Turnip's: the marker stays until the
    // editor has drawn its first frames (mark_healthy).
    let _ = std::fs::write(markers.join("pending"), format!("{:?}", std::time::SystemTime::now()));

    // Turnip's debug / tuning switches, read when the driver initialises
    // (libadrenotools' own constructor sets a TU_DEBUG of its choosing, so
    // ours goes in again after the load, before the initialisation).
    let tu_debug = std::env::var("RUSTUDIO_TU_DEBUG").ok().filter(|v| !v.trim().is_empty());
    let set_tu_debug = || {
        if let Some(v) = &tu_debug {
            // SAFETY: before any wgpu thread exists; android_main is the
            // only thread reading the environment at this point.
            #[allow(unused_unsafe)]
            unsafe {
                std::env::set_var("TU_DEBUG", v.trim())
            };
        }
    };
    set_tu_debug();

    let path = CString::new(shim.to_string_lossy().as_bytes()).map_err(|e| skip(e.to_string(), true))?;
    // SAFETY: dlopen of a library the APK ships; its constructor sets Mesa
    // environment variables and starts a thread that waits for this dlopen
    // to finish (libadrenotools-native src/driver.cpp).
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let _ = std::fs::remove_file(markers.join("pending"));
        return Err(skip(format!("dlopen {}: {}", shim.display(), dlerror()), true));
    }
    set_tu_debug();

    // The fork's own initialisation, the way System.loadLibrary would run
    // it, but here and now: it resolves nativeLibraryDir and the cache
    // folder through JNI, opens the hooked copy of libvulkan.so with the
    // Turnip driver behind it, and patches the system loader's
    // vkGetInstanceProcAddr / vkGetDeviceProcAddr. Its fallback thread
    // does the same a moment later when nobody calls this; a std::call_once
    // inside makes the two one, so by the time this returns the swap is
    // done (or has failed, which its logcat lines say).
    let vm = super::app().map(|a| a.vm_as_ptr()).unwrap_or(std::ptr::null_mut());
    if vm.is_null() {
        let _ = std::fs::remove_file(markers.join("pending"));
        return Err(skip("no JavaVM for libadrenotools' initialisation", true));
    }
    type JniOnLoad = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
    let on_load: JniOnLoad = match unsafe { symbol(handle, c"JNI_OnLoad") } {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(markers.join("pending"));
            return Err(skip(format!("libadrenotools.so without an entry point: {e}"), true));
        }
    };
    let t = Instant::now();
    // SAFETY: the library's JNI entry, given the process's JavaVM; it
    // attaches this thread itself when it has to.
    let jni_version = unsafe { on_load(vm, std::ptr::null_mut()) };
    log::info!("turnip: libadrenotools initialised (JNI {:#x}) in {:.0} ms", jni_version, t.elapsed().as_secs_f64() * 1e3);

    // Which driver answers now.
    let t = Instant::now();
    let probe = probe();
    let probe_ms = t.elapsed().as_secs_f64() * 1e3;
    let mut status = match probe {
        Ok(p) => {
            log::info!("turnip: probe: {} — {} (Vulkan {}) in {probe_ms:.0} ms", p.device, p.driver, version(p.api));
            Status {
                active: p.turnip,
                available: true,
                note: if p.turnip {
                    "Turnip answers this run's Vulkan calls".into()
                } else {
                    "libadrenotools didn't take over the loader (its logcat lines, tag AdrenoToolsPatch, say why); the device's driver is in use".into()
                },
                device: p.device,
                driver: p.driver,
            }
        }
        Err(e) => {
            // Either way the start goes on: wgpu makes its own attempt (and
            // has OpenGL ES to fall back to). The pending marker stays, so
            // a start that dies here keeps Turnip off next time.
            log::error!("turnip: the Vulkan probe after the swap failed: {e}");
            Status { active: false, available: true, note: format!("Vulkan probe after the swap failed: {e}"), device: String::new(), driver: String::new() }
        }
    };
    if !status.active && !status.device.is_empty() {
        // The device's driver is in use: nothing to guard.
        let _ = std::fs::remove_file(markers.join("pending"));
    }

    // The GPU's top clocks, on request (a KGSL property; thermal limits
    // still apply). Independent of which driver is in use.
    if std::env::var("RUSTUDIO_GPU_TURBO").map_or(false, |v| v.trim() == "1") {
        type SetTurbo = unsafe extern "C" fn(bool);
        match unsafe { symbol::<SetTurbo>(handle, c"adrenotools_set_turbo") } {
            // SAFETY: the library's documented entry; takes a bool.
            Ok(f) => {
                unsafe { f(true) };
                log::info!("turnip: adrenotools_set_turbo(true) (RUSTUDIO_GPU_TURBO=1)");
                status.note.push_str("; GPU turbo asked for");
            }
            Err(e) => log::warn!("turnip: RUSTUDIO_GPU_TURBO: {e}"),
        }
    }
    Ok(status)
}

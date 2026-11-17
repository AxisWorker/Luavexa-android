//! The desktop binary (Linux, macOS, Windows): everything is in the library
//! (src/lib.rs), shared with the Android `cdylib` in crates/rustudio-android.

// Launch as a GUI application on Windows without opening a console window.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    rustudio::desktop_main()
}

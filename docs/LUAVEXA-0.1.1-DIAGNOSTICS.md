# Luavexa Aurora 0.1.1 Alpha — Diagnostics & Identity

> **Status:** candidate build, pending GitHub Actions and device validation. It is not a verified FPS fix.

## What changed

- **Luavexa Debug** in **View → Luavexa Debug / GPU...** shows live estimated FPS, frame time, logical 3D viewport size, selected quality/resolution/detail, SSAO/shadows/bloom, and graphics adapter/driver details.
- The same debug window can be opened while **Play fills the screen** using the top-right **Debug** button, without exiting Play.
- **Experimental fast linear transparency** switch, **OFF by default**. When enabled it bypasses the encoded SeeThrough blend passes and draws with the existing linear pipelines. It is **session-only** and changes how transparency blends: colors and edges can differ. This is an A/B **diagnostic**, not the final visual-quality-preserving optimization.
- Buttons to reset the experiment, copy an FPS summary, and export crash/session logs on Android.
- App-facing branding is **Luavexa**, including the Android package ID `dev.luavexa.studio`, launcher label/icon, menus, About dialog, settings page, file-pickers, public log export directory, and APK artifact filenames.
- For normal pull requests, the Android workflow defaults to **one arm64 release** APK, without optional Mesa drivers. Manual runs can select x64/debug or extra drivers; tagged releases retain both architectures.

## Essential installation warning

Changing `dev.rustudio.studio` to **`dev.luavexa.studio`** makes Android treat this APK as a **different app**.

1. **Back up/export projects** from any previous Rustudio/Luavexa test installation before attempting updates or cleanup.
2. The new APK can coexist with the old one; internal settings, login and private app storage **will not automatically migrate**.
3. Never uninstall the original until you have verified access to the projects you need.
4. Permissions for the new package may need to be granted again.
5. The APK's native Rust library is still `librustudio_android.so` and Rust packages remain `rustudio-*`, by design. Renaming those identifiers is a separate code migration and not necessary for Luavexa branding.

Existing `RUSTUDIO_*` diagnostic/environment flags and internal resource paths remain stable for backwards-compatible builds and test harnesses. Upstream license notices and credits remain intact.

## Repeatable texture performance test

Test **the same phone, 3D resolution, quality preset, Play mode, camera direction and viewport size**. Use the normal 2048-stud Baseplate with its translucent top `Texture` (Transparency 0.8) as a test scene.

| Scenario | Linear transparency | Expected use |
| --- | --- | --- |
| Baseplate with Texture | OFF (baseline) | Measure original, visually accurate SeeThrough rendering |
| Baseplate with Texture | ON (experiment) | Compare FPS and identify possible blend-pass overhead |
| Baseplate without Texture | OFF | Control: isolate rendering of the Texture |
| Classic Baseplate | OFF | Control: different, simpler template |

Please record FPS after several seconds of stable camera framing; also note whether the Texture visually changes when the experiment is ON. Expect **no guaranteed speedup**.

Prior observations from the first Luavexa APK on Galaxy S9 SM-G9600, **not measured by CI**:

- Full-screen normal Baseplate with Texture at 25% resolution / Low preset: approximately 40–45 FPS.
- Same normal Baseplate after removing Texture: 60 FPS.
- Classic Baseplate: 60 FPS.
- Play → Edit had one unreproduced crash; root cause not established.

If the experiment improves FPS, investigate the exact GPU-pass cost before choosing an optimized default. If not, instrument the texture shader/rasterization separately. Do not remove user content as a performance shortcut.

## Crash diagnostics

1. Start Play and Stop normally a few times, without rapidly tapping Stop repeatedly.
2. If a crash occurs, reopen Luavexa and choose **View → Luavexa Debug / GPU... → Export crash & session logs to Download**, or **Help → Copy Logs to Download**.
3. Look for **Download/Luavexa/luavexa-logs-*.txt**.
4. Review logs for sensitive data before sharing. Do not include .ROBLOSECURITY cookies, session tokens, private scripts, or keys.

## Validation before merging

- [ ] CI `cargo check` and Android arm64 release artifact succeed.
- [ ] Package name in APK is `dev.luavexa.studio`; launcher shows Luavexa; original app remains separate.
- [ ] Debug panel opens in Edit and Play fullscreen; 48-point debug button is touchable.
- [ ] Frame counters update; copying summary and exporting logs work on device.
- [ ] The A/B toggle changes transparency path and restores the baseline without restarting.
- [ ] Compare normal Baseplate with Texture ON/OFF in both toggle modes at equal settings.
- [ ] Viewport, Explorer, Properties, files, scripts, save, Play and Stop still function.
- [ ] Retest intermittent Play-stop crash and inspect logs if reproduced.
- [ ] Permissions and external file opening work with the new package ID.
- [ ] Test that exported folders and project backups are accessible on Android 10.

## Out of scope

A permanent transparent-surface optimization and a full Aurora 0.2 editor redesign are deliberately postponed until this A/B evidence and stability tests are complete.

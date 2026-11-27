# Luavexa Aurora 0.1.2 — Mobile workspace fit and next GPU investigation

This update responds to the first real-device feedback on Luavexa 0.1.1, without changing rendering correctness or promising an unverified GPU speedup.

## Mobile layout regression

The new app ID (`dev.luavexa.studio`) created a fresh Android installation and a fresh layout. On the tested Galaxy S9 (2220 physical pixels across; native density ~1.54), the old 1100-point target led to auto zoom **1.30x**. The screenshot shows a cramped landscape editor ribbon and a narrow work area.

Changes proposed:
- Set the default Android landscape target to **1280 logical points** (expected first-run zoom approximately 1.10–1.15x on the same device).
- Add **View → Fit Workspace Zoom** to recompute the target on an existing installation without deleting settings, changing Android DPI, or touching project data.
- Make ribbon tools **horizontally scrollable on Android** so the rightmost tools can still be reached by swiping, without shrinking their touch target dimensions.
- Keep the existing View → Zoom Out/In options and persisted manual zoom.

The target is approximate: test actual touch size, orientation and text readability on device, including the user's current 700dp smallest-width developer setting. The first-run default does **not** override an existing saved zoom.

## Real-device performance observations (not CI measurements)

On an Adreno 630 (Qualcomm Vulkan driver; Android 10), with the normal translucent Baseplate `Texture`:

| Scenario | FPS reported | Notes |
| --- | --- | --- |
| Low preset, 50% 3D resolution, fullscreen Play, camera toward floor, A/B OFF | ~18 | Original SeeThrough path |
| Same scenario, A/B ON | ~17 | Experimental linear path |
| Same scenario, camera toward sky | 60 | Both paths |
| Earlier: normal Baseplate, 25% 3D resolution, Texture present | ~40–45 | Previous test |
| Earlier: same Baseplate, Texture removed | 60 | Control |
| Classic Baseplate | 60 | Simpler template control |

**Conclusion:** the experimental linear blending toggle was **not a performance fix** on this device. The 1 FPS difference is within plausible run-to-run noise. Keep **OFF** by default. The texture's visibility and projected screen coverage still correlate strongly with frame time. Rendering cost may be in pixel fill, texture sampling, fragment shading, offscreen passes, or driver behavior; these must be measured before modifying a shader or deleting a texture.

The uploaded session diagnostics reported **0 crash reports**, Vulkan Adreno 630, weak-GPU mode enabled, first-run zoom 1.30, and significant GPU allocations during Play. The export did **not** contain a per-pass GPU benchmark; avoid attributing exact milliseconds to any stage without evidence. The unrelated previous intermittent Play-stop crash remains unconfirmed.

## New: Luavexa Debug touch and opt-in GPU instrumentation

- **View → Luavexa Debug / GPU...**: drag the **title bar** directly on Android. Touches beginning on the title use a move gesture; the rest of the window remains scrollable. The position is constrained within the current screen.
- **Measure individual GPU passes (experimental)**: opt-in checkbox in the Debug window. The existing `crates/rustudio-render/src/gpu_timing.rs` now samples GPU pass durations asynchronously (when the Vulkan driver exposes both required timestamp-query features). This is disabled by default and disabled again when the window closes.
- GPU numbers show the largest **recorded spans** and can be copied. They are **not an end-to-end frame time**; UI composition and presentation may not be included. Measurements can add a small amount of overhead.
- Where timestamp-query features are unavailable, the panel explains that per-pass measurements are unsupported; FPS and GPU-driver details remain available.
- The existing CI `ui_glyphs` check now distinguishes bundled-editor-font requirements from two uncompiled legacy source files and explicit Roblox Studio **downloaded-font test fixtures**. It does not skip other unsupported UI glyphs.

## Next measurements

1. Keep **exactly the same camera, full screen resolution and graphics settings** when comparing.
2. Enable **Measure individual GPU passes** and compare **with/without** Baseplate Texture. If this device does not support timestamp queries, report that rather than guessing.
3. Inspect differences in scene opaque, see-through and postprocessing spans, without deleting users' content permanently.
4. The A/B linear transparency path did not improve the earlier test: leave it **OFF**.
5. Test frame time and touch behavior after the app has warmed up and Roblox content downloads have finished.

## Acceptance criteria

- [ ] ARM64 Release builds on GitHub Actions.
- [ ] Narrow landscape UI: visible menu, ribbon can swipe to last tool, panels remain usable.
- [ ] View → Fit Workspace Zoom changes the current zoom to a sensible fit and persists.
- [ ] Luavexa Debug title moves immediately on touch and its body scrolls.
- [ ] GPU timing switch is OFF initially, copies actual results when supported and reports unsupported drivers.
- [ ] CI `ui_glyphs` test passes on the updated candidate.
- [ ] No regression to desktop Studio ribbon or existing shortcut zoom.
- [ ] Play/Stop, project save, scripts and touch interaction work as previously.
- [ ] Debug A/B remains OFF by default; no FPS improvement is claimed from this update.

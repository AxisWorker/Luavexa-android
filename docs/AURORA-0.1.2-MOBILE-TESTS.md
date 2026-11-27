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

## Suggested next measurements

1. Keep **exactly the same camera, full screen resolution and graphics settings** when comparing.
2. Try **View → MicroProfiler** or the existing GPU timer/profiler infrastructure (`crates/rustudio-render/src/gpu_timing.rs`). GPU timestamp queries may be unavailable on some Android drivers; detect support and fall back to CPU/scene timings.
3. Compare the 3D scene before and after removing the Baseplate Texture, without changing environmental effects or camera.
4. Measure elapsed time in scene opaque, see-through drawing, postprocessing and UI passes where available; do not infer from FPS alone.
5. Fix the existing `ui_glyphs` CI failure separately; it reports missing bundled symbols even though the Android Release build succeeded.

## Acceptance criteria

- [ ] ARM64 Release builds on GitHub Actions.
- [ ] Narrow landscape UI: visible menu, ribbon can swipe to last tool, panels remain usable.
- [ ] View → Fit Workspace Zoom changes the current zoom to a sensible fit and persists.
- [ ] No regression to desktop Studio ribbon or existing shortcut zoom.
- [ ] Play/Stop, project save, scripts and touch interaction work as previously.
- [ ] Debug A/B remains OFF by default; no FPS improvement is claimed from this layout update.

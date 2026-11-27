# Luavexa Adreno 630 — experimental Low decal fast path

## Why
On Galaxy S9 Adreno 630 (Android 10), 50% render resolution, Manual/Low, normal Baseplate near SpawnPoint: GPU `see-through draws` 63.323 ms at 14.5 FPS. With the Baseplate overlay removed as reported: `see-through draws` 9.148 ms at 60.4 FPS. CPU+UI remained below 8ms. See PR #8 investigation.

## What this experiment changes
Only the WGSL `fs_part_enc` entrypoint, used for encoded see-through rendering. Ordinary Decal/Texture overlays on **Low shading detail** with **no special surface modifier** now go directly to `part_shade_low`, without executing the generic heavy material/surface path first. Uses identical texture sample, untiled edge clamp, sRGB conversion, per-vertex tint, transparency, sun/ambient/fog and encoded blending calls. Other materials, special surfaces, lighting settings and modes still use `part_shade` unchanged.

**This branch is experimental and not merged.** A possible optimization for a 2020 Qualcomm shader compiler, not a confirmed FPS improvement. The new branch cannot be activated by flipping the old 'fast linear transparency' A/B toggle; it changes internal shader branching at Low quality only.

## Validation on actual Galaxy S9
1. Keep the same Baseplate with its Texture/Decal, Play fullscreen, 3D 50%, Manual/Low, fast linear transparency OFF.
2. With the original Aurora 0.1.3 APK, record FPS and GPU see-through draws at the same camera position.
3. Install the experimental APK generated from this PR (same app ID, don't clear storage). Let shader pipeline warm up, then record FPS and GPU see-through draws.
4. Compare the baseplate checkered texture, SpawnLocation icon, transparency edges, fog and lighting in screenshots. Try Transparency=0 and 0.8 for both Texture and Decal in a disposable test copy.
5. Test Play Stop and character controls. The separate known AudioTrack close crash is not addressed in this shader PR.
6. Record both the build and shader comparisons before considering merge. If gain is absent or appearance differs, do not merge.

## Acceptance criteria
- [ ] Android Release builds and shader validates on Vulkan Adreno 630.
- [ ] Visible output close to Aurora 0.1.3 baseline on Test Place, Baseplate and custom textures.
- [ ] GPU `see-through draws` is materially lower at the same scene/camera or otherwise mark experiment inconclusive.
- [ ] No changes to user data, GPU driver selection, resolution, settings defaults or audio logic.
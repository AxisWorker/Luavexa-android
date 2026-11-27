# Luavexa Aurora — Adreno 630 FPS and Play Stop crash evidence

## A. Performance: measured GPU hotspot

Tested on Galaxy S9, Adreno 630, Play fullscreen, 3D 50%, Manual/Low, encoded transparency mode (A/B false), same viewport 1444 × 702 points and near SpawnLocation.

| User test | FPS | Frame interval | CPU+UI | GPU see-through draws | GPU opaque scene |
| --- | ---: | ---: | ---: | ---: | ---: |
| Normal Baseplate | 14.5 | 69.0 ms | 7.551 ms | **63.323 ms** | 1.966 ms |
| Without the Baseplate decal, according to user wording | 60.4 | 16.6 ms | 5.608 ms | **9.148 ms** | 1.963 ms |
| 'Sem decal baseplate e spawn : Baseplate Transparent 0' | 14.0 | 71.7 ms | 5.986 ms | **64.750 ms** | 2.799 ms |

Note that the third test's wording is ambiguous; confirm the specific Texture/Decal instances changed before treating it as an isolated alpha=0 experiment.

**Conclusion:** the expensive pass is GPU translucent/decal fragment drawing, **not** the see-through encode/decode postprocessing passes (~0.2–0.3 ms each). Roughly 55 ms of pass cost varies with visible textured overlays. CPU+UI is 5–8 ms in the same cases. This is the best grounded root-cause localization so far.

Implementation evidence:
- crates/rustudio-render/src/render.rs sorts decals as translucent draws and emits the 'gpu: see-through draws' timestamp around the render pass.
- crates/rustudio-render/src/shader.wgsl routes encoded transparent passes via fs_part_enc to general part_shade. Fragment math includes texture sampling, sRGB conversion (pow), shading and gamma encoding. Which individual operation causes the driver bottleneck remains a hypothesis.

### Separate graphics optimization follow-up
Specialize the decal/Texture shader path for mobile Low quality behind a session-only A/B experiment with unchanged default and image-quality comparison. Preserve UV tiling, alpha edge appearance, lighting, depth ordering, fog and original SeeThrough look. Build Android ARM64 Release and compare the identical camera, GPU spans, FPS and screenshots before enabling by default.

## B. Native audio crash after Play Stop

User upload 'luavexa-logs-20261127-160431.txt' records:
- 16:04:24.972: Play Stop completed.
- 16:04:25.669: native AAudioStream_close(s#5) began.
- 16:04:25.675: AudioTrack callback reported 'Pure virtual function called!'.
- 16:04:25.678: process aborted with signal 6 (SIGABRT), in AudioTrack thread.
- Android tombstone includes libaaudio.so AudioStreamLegacy::processCallbackCommon and libaudioclient.so AudioTrack::processAudioBuffer.
- Earlier, at 16:00:56 and 16:03:00, AudioTrack reported stream disconnections during audio device changes.

The rodio/cpal output is opened lazily and closed at Play teardown. This strongly implicates a stream-close/callback lifetime race on Android 10, but the precise fault is not yet proven. Investigate and test a separate audio lifecycle fix; do not change shader code or disable sounds as a workaround.

## C. User-visible brand migration

UI descriptions and crash notices now say Luavexa. Saved technical IDs (ToolImpl::Rustudio = 'rustudio'), compatibility folders, native crate names and RUSTUDIO_* environment variables are intentionally unchanged. Old crash reports remain readable, but future reports display Luavexa.

Toolbox sign-in is Roblox authentication, not a legacy Rustudio name.
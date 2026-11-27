# Luavexa Aurora 0.1.3 — Android frame pacing investigation

## Observation (Galaxy S9, Android 10, Qualcomm Adreno 630)

The user's Play test now has working player controls, camera and R15 scripts. Fullscreen view of the normal Baseplate / SpawnLocation still falls to about **20 FPS** in some camera views, while sky-only views can reach **60 FPS**.

The user copied these GPU timestamps from **one latest resolved frame** (camera orientation at capture was not specified):

| Timed pass | Duration |
| --- | ---: |
| GPU scene (opaque / terrain / unencoded blends) | 3.491 ms |
| Post | 0.692 ms |
| Skinning | 0.529 ms |
| SSAO, clouds, environment probe, DOF, shadows, bloom | 0.013 ms combined |

The listed spans total about **4.725 ms** (not the total GPU frame time). **20 FPS represents ~50 ms average wall-clock frame interval**, but the gap **cannot be attributed to one component** by subtracting timestamps: timestamps can be resolved from a different frame, execution can overlap, and the existing profiler does not time the egui draw/composition, queue stalls or window presentation.

The GPU timing module explicitly documents these limitations (`crates/rustudio-render/src/gpu_timing.rs`).

## What this PR adds

The existing **Luavexa Debug** panel now shows:

- `FPS / wall frame interval` — smoothed, from frame scheduling.
- `CPU update + UI` — the preceding frame's `Editor::finish_frame` elapsed CPU time, **before eframe submission and window present**.
- `GPU pass timing` — existing asynchronous timestamp queries, where supported, with explicit coverage notes.
- Copy actions that include CPU update time and caution about missing presentation time.

This is **diagnostic only**. It does **not** disable textures, modify shaders, change FPS pacing or alter driver selection. Instrumentation may add overhead, so keep the detailed GPU option disabled unless recording a controlled comparison.

## On-device test matrix

Use the exact same settings (e.g. fullscreen, Low, 50% 3D resolution, transparency A/B OFF) and let each camera view settle before noting the FPS and CPU time. For each case, copy the complete **GPU timings** rather than comparing a single unstable frame.

1. Baseplate **with** texture, looking at **sky**.
2. Baseplate **with** texture, looking **down at the floor**.
3. Baseplate **without** Texture, looking down from the **same angle and position**.
4. Same place without Baseplate Texture, near SpawnLocation, then temporarily hide only the **SpawnLocation → Decal** in a disposable test copy.
5. As a control, **Classic Baseplate**, same resolution and renderer quality.
6. Compare **Edit** and **Play** separately. Play now loads Roblox CoreScripts and can have much more CPU work.

**Interpretation**:
- If `CPU update + UI` rises to approximately 40–50 ms during 20 FPS, examine CoreScripts / Lua, scene sync, input and UI build on the CPU.
- If CPU stays far below 50 ms and measured scene GPU spans stay short, inspect **egui rendering**, GPU command queue and Android Vulkan **swapchain / present / vsync** next. This is a hypothesis, not proof of a bad driver.
- If the timed `gpu: scene` rises strongly when the Decal or Texture covers the view, profile its fragment pipelines. Don't eliminate real assets as a production fix.

## Separately discovered crash — audio / Play Stop

The uploaded `luavexa-logs-20261127-131102.txt` shows one **SIGABRT** at **13:08:34** on an Android **AudioTrack** thread, about 2 seconds after a Play Stop at 13:08:32. The tombstone contains **`Pure virtual function called!`** while `AAudioStream_close` was running, through the platform's `AudioStreamLegacy::processCallbackCommon` / `AudioTrack::processAudioBuffer`. A later Play Stop in the same log returned from AAudio close normally.

This is consistent with a **race during audio stream callback teardown**, not an established GPU frame-time issue. The report should be tracked independently: review `crates/rustudio-engine/src/audio.rs` output-sink lifetime, cpal/rodio on Android 10, and safe shutdown ordering. Do not silently disable game sound to hide the bug.

## Acceptance criteria

- [ ] Android ARM64 Release compiles.
- [ ] Luavexa Debug still works in normal editing and fullscreen Play.
- [ ] CPU update+UI displays sane nonnegative values and updates during Play.
- [ ] Copy GPU timings includes CPU update time and accurately notes excluded stages.
- [ ] No shader or quality changes; no claim of increased FPS.
- [ ] Re-run controlled Play / Edit / sky / floor / Decal tests before proposing a rendering optimization.

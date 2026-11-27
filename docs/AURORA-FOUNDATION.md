# Luavexa 0.1 Alpha — Project Aurora Foundation

> **Status:** initial UI implementation on a feature branch; not a tested release. The dedicated Luavexa AI chat is outside this milestone.

## Goal

Give the native Android editor a distinct Luavexa visual identity **without replacing or destabilizing** the inherited Rustudio editor and project workflows.

## Implemented in this branch

- Aurora palette in `crates/rustudio-engine/src/theme.rs`: navy surfaces, purple primary actions, cyan selection, readable text, softer radii and spacing.
- The approved Luavexa 256px icon replaces the embedded Rustudio chrome/window icon (`src/icons.rs`).
- The existing `src/start_page.rs` keeps its template, Roblox cloud, recent project, folder and file workflows.
- The new Home hero presents the Luavexa brand and direct New/Open project actions.
- At widths below **760 egui points**, the fixed 220-point sidebar is replaced by touch-friendly **48-point** action buttons; search moves to the content area.
- Optional card hover/hero transitions use short existing egui animation helpers; a **Reduce motion** checkbox disables those optional Home animations **during the current editor session**.
- No shader blur, new image effects, added packages, or custom animation engine.

## Design tokens (Aurora v0.1)

| Role | Color |
| --- | --- |
| Background | `#0B1025` |
| Main panel | `#121931` |
| Raised panel | `#151B3E` |
| Accent purple | `#8A49F7` |
| Selection cyan | `#00D9F5` |
| Primary text | `#E8EDFF` |
| Secondary text | `#A9B5D2` |

The intention is **solid surfaces with subtle highlights**, not live glass blur. Contrast and mobile behavior need validation on actual displays.

## What remains for 0.2

A deeper toolbar, Explorer, Properties, output, script-tab, and panel layout redesign is explicitly **not** part of this 0.1 foundation. The in-app AI, MCP front-end integration, skill routing and reference retrieval come later, after editor stability and build checks.

## Verification / acceptance checklist

- [ ] GitHub Actions Android ARM64 build passes and produces an installable APK.
- [ ] Install and open successfully on an Android 10 ARM64 device, e.g. Galaxy S9.
- [ ] Home works portrait and landscape; no clipped New/Open/Folder or search actions.
- [ ] New template, open place file, and open project folder work as they did upstream.
- [ ] Project scripts, Explorer, Properties, output, Play, save, and undo still work.
- [ ] Home card hover animation terminates after a short transition; no perpetual redraw.
- [ ] Reduced-motion toggle visibly disables Home transitions.
- [ ] UI remains readable at Android font/display scaling.
- [ ] Measure startup, input latency, CPU/GPU load, and memory in representative projects.
- [ ] Check existing installation/signature/package-ID behavior before updating a previous build.

Do not mark these checks as passed unless observed. UI tests and a successful APK are separate validation milestones.

## Technical guardrails

- The Android application ID remains `dev.rustudio.studio` for this patch, intentionally.
- Existing Rust crate names and native Android entry points remain unchanged.
- Preserve MIT OR Apache-2.0 notices and upstream attribution.
- No new Android permissions, remote telemetry, model provider connections, or background services.
- Do not merge without reviewing CI outcomes and basic interface screenshots.

## Quick reference

- [Android build and testing](ANDROID.md)
- [Developer harness](DEVELOPING.md)
- [Project engineering rules](../AGENTS.md)
- [Known engine limitations](ENGINE-GAPS.md)

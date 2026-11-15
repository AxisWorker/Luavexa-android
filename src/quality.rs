//! Graphics quality. One struct ([`RenderSettings`]) says what the 3D view
//! spends its GPU on; a quality level, 1 (cheapest) to 10, fills it in
//! from the tables in src/mech.rs (`QUALITY_*`), as Studio's Settings
//! slider does over its QualityLevel Level01..Level21; level 10 is
//! Studio's defaults. Over the level sit the individual settings
//! ([`Overrides`]: render distance, texture quality, anti-aliasing, the
//! shadows, the post passes…), each "Auto" — from the level — unless set
//! (View > Graphics Settings, src/graphics_settings.rs). The whole is
//! [`Graphics`], kept across runs by the layout store (layout_store.rs)
//! and mirrored in Studio Settings > Rendering > Editor Quality Level
//! (`settings().Rendering.EditQualityLevel`, src/studio_settings.rs).
//!
//! Automatic (`Graphics::auto`, Studio's QualityLevel.Automatic) moves the
//! level by the frame rate ([`Auto`]): down a level after two seconds of
//! frames well over a 60 Hz refresh, up a level after twenty seconds at
//! it, never back up to a level it just left for a minute and a half, and
//! only while the editor is drawing frames back to back (a game running,
//! the camera moving: an idle editor repaints on input and its long
//! frames are pauses); the first half second after a change, pipelines
//! and targets being remade, isn't counted. A phone starts in Automatic
//! at level 3, a computer at level 10 (`Graphics::platform_default`).
//!
//! The renderer (render.rs) makes only the pipelines and render targets
//! the settings call for and skips the passes they turn off; the viewport
//! sizes its target by `render_scale`; parts and terrain beyond
//! `view_distance` and parts smaller than `detail_px` are not drawn; the
//! material texture arrays are `texture_texels` across.
//! `RUSTUDIO_QUALITY=auto|low|medium|high|1..10` and the per-field
//! `RUSTUDIO_*` switches ([`RenderSettings::with_env`]) override for a
//! run (on Android through `adb shell setprop debug.rustudio.env`,
//! docs/ANDROID.md).
//!
//! What each field costs on a phone's GPU (a tiler with a tenth of a
//! desktop card's bandwidth): MSAA multiplies the colour and depth kept
//! per pixel in tile memory (4× plus a 64-bit HDR colour leaves an
//! Adreno's tiles a few thousand pixels, so the scene is binned and
//! drawn many times over); the 64-bit HDR target doubles every
//! full-screen read and write against 32-bit RG11B10 float; bloom, SSAO
//! and depth of field are each three to fourteen full- or half-screen
//! passes; each sun cascade and each shadowed light is a pass over the
//! casters plus blurs; every part drawn is a vertex load and a fill, and
//! the far, small ones add nothing a phone's screen shows; a 1024² material
//! layer is 12 MB with its mips, three arrays of them per material.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui_wgpu::wgpu;
use serde::{Deserialize, Serialize};

use crate::mech;

/// Studio's Settings slider: 1 to this.
pub const LEVELS: u8 = mech::QUALITY_LEVELS;

/// Our level for Studio's QualityLevel enum value (0 Automatic, 1..=21
/// Level01..Level21: `settings().Rendering.EditQualityLevel`).
pub fn level_from_studio(n: u32) -> Option<u8> {
    if n == 0 {
        return None;
    }
    Some(((n.min(21) * LEVELS as u32).div_ceil(21) as u8).clamp(1, LEVELS))
}

/// Studio's QualityLevel value for our level (Level02 for 1 … Level21 for
/// 10; `level_from_studio` gives the level back).
pub fn level_to_studio(level: u8) -> u32 {
    (level.clamp(1, LEVELS) as u32 * 21) / LEVELS as u32
}

/// `RUSTUDIO_QUALITY` and the layout store's spelling: `auto`
/// (`Ok(None)`), `low` / `medium` / `high` (levels 3 / 6 / 10) or a
/// level number.
pub fn parse_level(s: &str) -> Option<Option<u8>> {
    match s.trim().to_ascii_lowercase().as_str() {
        "auto" | "automatic" | "a" => Some(None),
        "low" | "l" => Some(Some(3)),
        "medium" | "med" | "m" => Some(Some(6)),
        "high" | "h" => Some(Some(LEVELS)),
        n => n.parse::<u8>().ok().filter(|n| (1..=LEVELS).contains(n)).map(Some),
    }
}

/// The sun's shadow maps as one setting: cascades and texel scale.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Shadows {
    /// No shadow maps: the voxel sun alone (Lighting.GlobalShadows off).
    Off,
    /// One cascade at half the texels.
    Low,
    /// Two cascades at three quarters.
    Medium,
    /// Studio's four cascades at its sizes.
    High,
}

impl Shadows {
    pub const ALL: [Shadows; 4] = [Shadows::Off, Shadows::Low, Shadows::Medium, Shadows::High];

    pub fn name(self) -> &'static str {
        match self {
            Shadows::Off => "Off",
            Shadows::Low => "Low",
            Shadows::Medium => "Medium",
            Shadows::High => "High",
        }
    }

    /// (cascades, texel scale).
    pub fn cascades(self) -> (u32, f32) {
        match self {
            Shadows::Off => (0, 0.5),
            Shadows::Low => (1, 0.5),
            Shadows::Medium => (2, 0.75),
            Shadows::High => (crate::sun_shadow::CASCADES as u32, 1.0),
        }
    }

    /// The nearest name for a cascade count and scale (the level tables').
    pub fn nearest(cascades: u32, scale: f32) -> Shadows {
        match cascades {
            0 => Shadows::Off,
            1 => Shadows::Low,
            2 if scale <= 0.8 => Shadows::Medium,
            _ => Shadows::High,
        }
    }
}

/// The individual settings: `None` follows the level ("Auto"). Saved in
/// the layout file (layout_store.rs) as JSON, so no infinities: an
/// unlimited view distance is `Some(0.0)`.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Overrides {
    /// Studs; 0 (or less) draws everything.
    pub view_distance: Option<f32>,
    /// Pixels: the smallest part drawn.
    pub detail_px: Option<f32>,
    /// The material texture arrays' side: 256, 512 or 1024.
    pub texture_texels: Option<u32>,
    pub msaa: Option<u32>,
    pub hdr: Option<bool>,
    pub render_scale: Option<f32>,
    pub shadows: Option<Shadows>,
    pub light_shadows: Option<bool>,
    pub ssao: Option<bool>,
    pub bloom: Option<bool>,
    pub dof: Option<bool>,
    pub clouds: Option<bool>,
    pub env_probe: Option<bool>,
}

impl Overrides {
    pub fn any(&self) -> bool {
        *self != Overrides::default()
    }
}

/// The user's graphics choices: the level, whether it moves on its own,
/// and the settings set apart from it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Graphics {
    /// 1..=LEVELS. In Automatic, the level the controller is at now.
    pub level: u8,
    /// Studio's QualityLevel.Automatic: [`Auto`] moves `level`.
    pub auto: bool,
    pub over: Overrides,
}

impl Graphics {
    /// Automatic from level `QUALITY_DEFAULT_PHONE` on Android, level 10
    /// elsewhere; `RUSTUDIO_QUALITY` overrides (and then wins over the
    /// saved layout: layout_store.rs).
    pub fn platform_default() -> Graphics {
        let mut g = if cfg!(target_os = "android") {
            Graphics { level: mech::QUALITY_DEFAULT_PHONE, auto: true, over: Overrides::default() }
        } else {
            Graphics { level: LEVELS, auto: false, over: Overrides::default() }
        };
        if let Some(env) = Self::env_level() {
            g.apply_choice(env);
        }
        g
    }

    /// `RUSTUDIO_QUALITY`, when set and understood.
    pub fn env_level() -> Option<Option<u8>> {
        std::env::var("RUSTUDIO_QUALITY").ok().and_then(|v| parse_level(&v))
    }

    /// `None`: Automatic (from the level it is at); `Some(n)`: level n, fixed.
    pub fn apply_choice(&mut self, choice: Option<u8>) {
        match choice {
            None => self.auto = true,
            Some(n) => {
                self.auto = false;
                self.level = n.clamp(1, LEVELS);
            }
        }
    }

    /// The choice as `RUSTUDIO_QUALITY` / the layout store spell it.
    pub fn choice_name(&self) -> String {
        if self.auto { "auto".to_string() } else { self.level.to_string() }
    }

    /// What the renderer draws with: the level's row, the overrides, then
    /// the run's `RUSTUDIO_*` switches (a device run's comparison wins
    /// over the saved choices).
    pub fn settings(&self) -> RenderSettings {
        RenderSettings::level(self.level).with_overrides(&self.over).with_env().normalized()
    }

    /// Studio's QualityLevel value for the choice (the Studio Settings mirror).
    pub fn studio_level(&self) -> u32 {
        if self.auto { 0 } else { level_to_studio(self.level) }
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::platform_default()
    }
}

/// What the renderer draws and with what. Fields marked (rebuild) change
/// the pipelines or the render targets: the renderer remakes them when
/// one changes (`Renderer::set_settings`); (textures) remakes the material
/// arrays; the others take effect on the next frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RenderSettings {
    /// The level these started from.
    pub level: u8,
    /// Studs from the camera beyond which parts and terrain chunks wholly
    /// farther aren't drawn (`f32::INFINITY`: all are).
    pub view_distance: f32,
    /// A part whose bounding radius covers fewer pixels is skipped.
    pub detail_px: f32,
    /// The material texture arrays' side, 256 ..= matpack::LAYER_SIZE
    /// (textures).
    pub texture_texels: u32,
    /// Scene multisampling: 1 (off), 2 or 4 (rebuild).
    pub msaa: u32,
    /// The scene in 64-bit Rgba16Float (true) or 32-bit Rg11b10Ufloat
    /// where the device renders to it, else Rgba16Float (rebuild). Both
    /// hold linear light above 1: exposure and the tone curve stay in
    /// the post pass.
    pub hdr: bool,
    /// The 3D view's pixels per panel pixel, 0.25 ..= 1 (the viewport
    /// sizes the target by it).
    pub render_scale: f32,
    /// The sun's shadow cascades drawn, 0 (no shadow maps: the voxel sun
    /// alone, as Lighting.GlobalShadows off) ..= 4. Points outside the
    /// drawn cascades take the voxel sun, as beyond the last cascade.
    pub sun_cascades: u32,
    /// The cascades' texels as a fraction of Studio's sizes, 0.25 ..= 1
    /// (sun_shadow::texel_scale; rebuild).
    pub shadow_scale: f32,
    /// Local lights' cube shadow maps (Studio's Future lights; rebuild:
    /// the atlas is allocated only when on).
    pub light_shadows: bool,
    /// Screen-space ambient occlusion.
    pub ssao: bool,
    /// Bloom pyramid levels at most, 0 = no bloom passes. Studio draws
    /// 3 to 5 by the view's size and the bloom's size.
    pub bloom_levels: u32,
    /// DepthOfFieldEffect passes (when the place has one).
    pub dof: bool,
    /// Terrain.Clouds' volumetric march and composite.
    pub clouds: bool,
    /// The scene environment probe's captures (env_probe.rs): without,
    /// reflections take the sky and Studio's indoor cube.
    pub env_probe: bool,
}

impl RenderSettings {
    /// The tables' row for a level (src/mech.rs `QUALITY_*`).
    pub fn level(level: u8) -> Self {
        let level = level.clamp(1, LEVELS);
        let i = level as usize - 1;
        RenderSettings {
            level,
            view_distance: mech::QUALITY_VIEW_DISTANCE[i],
            detail_px: mech::QUALITY_DETAIL_PX[i],
            texture_texels: mech::QUALITY_TEXTURE_TEXELS[i],
            msaa: mech::QUALITY_MSAA[i],
            hdr: level >= mech::QUALITY_HDR_FROM,
            render_scale: mech::QUALITY_RENDER_SCALE[i],
            sun_cascades: mech::QUALITY_SUN_CASCADES[i],
            shadow_scale: mech::QUALITY_SHADOW_SCALE[i],
            light_shadows: level >= mech::QUALITY_LIGHT_SHADOWS_FROM,
            ssao: level >= mech::QUALITY_SSAO_FROM,
            bloom_levels: mech::QUALITY_BLOOM_LEVELS[i],
            dof: level >= mech::QUALITY_DOF_FROM,
            clouds: level >= mech::QUALITY_CLOUDS_FROM,
            env_probe: level >= mech::QUALITY_ENV_PROBE_FROM,
        }
        .normalized()
    }

    /// The individual settings over the level's.
    pub fn with_overrides(mut self, o: &Overrides) -> Self {
        if let Some(v) = o.view_distance {
            self.view_distance = v;
        }
        if let Some(v) = o.detail_px {
            self.detail_px = v;
        }
        if let Some(v) = o.texture_texels {
            self.texture_texels = v;
        }
        if let Some(v) = o.msaa {
            self.msaa = v;
        }
        if let Some(v) = o.hdr {
            self.hdr = v;
        }
        if let Some(v) = o.render_scale {
            self.render_scale = v;
        }
        if let Some(s) = o.shadows {
            let (cascades, scale) = s.cascades();
            self.sun_cascades = cascades;
            self.shadow_scale = scale;
        }
        if let Some(v) = o.light_shadows {
            self.light_shadows = v;
        }
        if let Some(v) = o.ssao {
            self.ssao = v;
        }
        if let Some(v) = o.bloom {
            self.bloom_levels = if v { self.bloom_levels.max(3) } else { 0 };
        }
        if let Some(v) = o.dof {
            self.dof = v;
        }
        if let Some(v) = o.clouds {
            self.clouds = v;
        }
        if let Some(v) = o.env_probe {
            self.env_probe = v;
        }
        self.normalized()
    }

    /// The platform's graphics with the run's environment applied.
    pub fn platform_default() -> Self {
        Graphics::platform_default().settings()
    }

    /// The `RUSTUDIO_*` switches over these settings: `RUSTUDIO_MSAA`
    /// (1, 2, 4), `RUSTUDIO_HDR` (0|1), `RUSTUDIO_RENDER_SCALE` (0.25 to
    /// 1), `RUSTUDIO_SUN_SHADOWS` (0: no cascades), `RUSTUDIO_SUN_CASCADES`
    /// (0 to 4), `RUSTUDIO_SHADOW_SCALE` (0.25 to 1), `RUSTUDIO_LIGHT_SHADOWS`
    /// (0|1), `RUSTUDIO_SSAO` (0|1), `RUSTUDIO_BLOOM` (levels, 0 off),
    /// `RUSTUDIO_DOF` (0|1), `RUSTUDIO_CLOUDS` (0|1), `RUSTUDIO_NO_ENV_PROBE`,
    /// `RUSTUDIO_VIEW_DISTANCE` (studs, 0 unlimited), `RUSTUDIO_DETAIL_PX`,
    /// `RUSTUDIO_TEXTURE_TEXELS` (256, 512, 1024).
    pub fn with_env(mut self) -> Self {
        let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let flag = |k: &str| var(k).map(|v| v != "0" && !v.eq_ignore_ascii_case("off") && !v.eq_ignore_ascii_case("false"));
        let num = |k: &str| var(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
        if let Some(n) = num("RUSTUDIO_MSAA") {
            self.msaa = n as u32;
        }
        if let Some(on) = flag("RUSTUDIO_HDR") {
            self.hdr = on;
        }
        if let Some(s) = num("RUSTUDIO_RENDER_SCALE") {
            self.render_scale = s;
        }
        if flag("RUSTUDIO_SUN_SHADOWS") == Some(false) {
            self.sun_cascades = 0;
        }
        if let Some(n) = num("RUSTUDIO_SUN_CASCADES") {
            self.sun_cascades = n.max(0.0) as u32;
        }
        if let Some(s) = num("RUSTUDIO_SHADOW_SCALE") {
            self.shadow_scale = s;
        }
        if let Some(on) = flag("RUSTUDIO_LIGHT_SHADOWS") {
            self.light_shadows = on;
        }
        if let Some(on) = flag("RUSTUDIO_SSAO") {
            self.ssao = on;
        }
        if let Some(n) = num("RUSTUDIO_BLOOM") {
            self.bloom_levels = n.max(0.0) as u32;
        }
        if let Some(on) = flag("RUSTUDIO_DOF") {
            self.dof = on;
        }
        if let Some(on) = flag("RUSTUDIO_CLOUDS") {
            self.clouds = on;
        }
        if std::env::var_os("RUSTUDIO_NO_ENV_PROBE").is_some() {
            self.env_probe = false;
        }
        if let Some(d) = num("RUSTUDIO_VIEW_DISTANCE") {
            self.view_distance = if d <= 0.0 { f32::INFINITY } else { d };
        }
        if let Some(px) = num("RUSTUDIO_DETAIL_PX") {
            self.detail_px = px;
        }
        if let Some(t) = num("RUSTUDIO_TEXTURE_TEXELS") {
            self.texture_texels = t.max(1.0) as u32;
        }
        self.normalized()
    }

    /// Fields within their ranges.
    pub fn normalized(mut self) -> Self {
        self.level = self.level.clamp(1, LEVELS);
        self.view_distance = if self.view_distance.is_nan() || self.view_distance <= 0.0 { f32::INFINITY } else { self.view_distance.max(64.0) };
        self.detail_px = if self.detail_px.is_finite() { self.detail_px.clamp(0.0, 32.0) } else { mech::LOD_MIN_PX };
        self.texture_texels = self.texture_texels.clamp(256, crate::matpack::LAYER_SIZE).next_power_of_two().min(crate::matpack::LAYER_SIZE);
        self.msaa = match self.msaa {
            0 | 1 => 1,
            2 | 3 => 2,
            _ => 4,
        };
        self.render_scale = if self.render_scale.is_finite() { self.render_scale.clamp(0.25, 1.0) } else { 1.0 };
        self.sun_cascades = self.sun_cascades.min(crate::sun_shadow::CASCADES as u32);
        self.shadow_scale = if self.shadow_scale.is_finite() { self.shadow_scale.clamp(0.25, 1.0) } else { 1.0 };
        self.bloom_levels = self.bloom_levels.min(5);
        self
    }

    /// The settings that shape the pipelines and the render targets: equal
    /// keys share a renderer build.
    pub fn build_key(&self) -> (u32, bool, u32, bool) {
        (self.msaa, self.hdr, (self.shadow_scale * 100.0).round() as u32, self.light_shadows)
    }

    /// The scene's colour format on `device`: Rgba16Float for HDR, else
    /// Rg11b10Ufloat where the device renders to it (wgpu's
    /// RG11B10UFLOAT_RENDERABLE: every Vulkan phone GPU of the last
    /// decade, every desktop), else Rgba16Float after all.
    pub fn scene_format(&self, device: &wgpu::Device) -> wgpu::TextureFormat {
        if !self.hdr && device.features().contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE) {
            wgpu::TextureFormat::Rg11b10Ufloat
        } else {
            wgpu::TextureFormat::Rgba16Float
        }
    }

    /// One line for the perf log and the startup log.
    pub fn describe(&self, scene_format: wgpu::TextureFormat) -> String {
        let onoff = |b: bool| if b { "on" } else { "off" };
        format!(
            "level {}: view distance {}, detail {} px, textures {}², MSAA {}, scene {}, render scale {:.2}, sun cascades {} at x{:.2}, light shadows {}, AO {}, bloom {}, DoF {}, clouds {}, env probe {}",
            self.level,
            if self.view_distance.is_finite() { format!("{:.0} studs", self.view_distance) } else { "unlimited".to_string() },
            self.detail_px,
            self.texture_texels,
            if self.msaa > 1 { format!("{}x", self.msaa) } else { "off".to_string() },
            match scene_format {
                wgpu::TextureFormat::Rgba16Float => "RGBA16F",
                wgpu::TextureFormat::Rg11b10Ufloat => "RG11B10F",
                _ => "other",
            },
            self.render_scale,
            self.sun_cascades,
            self.shadow_scale,
            onoff(self.light_shadows),
            onoff(self.ssao),
            if self.bloom_levels > 0 { format!("{} levels", self.bloom_levels) } else { "off".to_string() },
            onoff(self.dof),
            onoff(self.clouds),
            onoff(self.env_probe),
        )
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self::platform_default()
    }
}

// ------------------------------------------------------------------ automatic

/// Automatic quality: the level follows the frame rate (the module
/// comment). Fed every frame ([`Auto::frame`]); says when the level
/// should move.
#[derive(Debug)]
pub struct Auto {
    /// Frame times (ms) of frames drawn back to back, with when they ended.
    frames: VecDeque<(Instant, f32)>,
    last_change: Option<Instant>,
    /// A level stepped down from, and until when it isn't gone back to.
    hold: Option<(u8, Instant)>,
}

impl Default for Auto {
    fn default() -> Self {
        Auto { frames: VecDeque::new(), last_change: None, hold: None }
    }
}

/// Why the level moved (the log).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Down,
    Up,
}

impl Auto {
    /// A frame of `dt` seconds since the last. Returns the step to take
    /// from `level`, if any; the caller changes the level and logs.
    pub fn frame(&mut self, now: Instant, dt: f32, level: u8) -> Option<Step> {
        let ms = dt * 1000.0;
        if !(ms > 0.0) || ms > mech::QUALITY_AUTO_IDLE_MS {
            // An idle pause (or the first frame): the frames before it
            // say nothing about now.
            self.frames.clear();
            return None;
        }
        let since_change = self.last_change.map_or(f32::INFINITY, |t| now.duration_since(t).as_secs_f32());
        if since_change < mech::QUALITY_AUTO_SETTLE_S {
            // (pipelines and targets remade for the new level: not its frames)
            return None;
        }
        self.frames.push_back((now, ms));
        let keep = Duration::from_secs_f32(mech::QUALITY_AUTO_UP_S);
        while self.frames.front().is_some_and(|(t, _)| now.duration_since(*t) > keep) {
            self.frames.pop_front();
        }
        if self.hold.is_some_and(|(_, until)| now >= until) {
            self.hold = None;
        }
        let budget = mech::QUALITY_AUTO_FRAME_MS;
        // Down: the last QUALITY_AUTO_DOWN_S of frames, their median long.
        if level > 1 && since_change >= mech::QUALITY_AUTO_DOWN_S {
            let span = Duration::from_secs_f32(mech::QUALITY_AUTO_DOWN_S);
            let recent: Vec<f32> = self.frames.iter().filter(|(t, _)| now.duration_since(*t) <= span).map(|(_, ms)| *ms).collect();
            let covered: f32 = recent.iter().sum::<f32>() / 1000.0;
            if recent.len() >= 20 && covered >= mech::QUALITY_AUTO_DOWN_S * 0.8 && median(&recent) > budget * mech::QUALITY_AUTO_SLOW {
                self.last_change = Some(now);
                self.hold = Some((level, now + Duration::from_secs_f32(mech::QUALITY_AUTO_HOLD_S)));
                self.frames.clear();
                return Some(Step::Down);
            }
        }
        // Up: QUALITY_AUTO_UP_S of frames back to back, their median at
        // the refresh, and not into a level just left.
        if level < LEVELS && since_change >= mech::QUALITY_AUTO_UP_S && !self.hold.is_some_and(|(held, _)| level + 1 >= held) {
            let all: Vec<f32> = self.frames.iter().map(|(_, ms)| *ms).collect();
            let covered: f32 = all.iter().sum::<f32>() / 1000.0;
            if all.len() >= 200 && covered >= mech::QUALITY_AUTO_UP_S * 0.8 && median(&all) <= budget * mech::QUALITY_AUTO_FAST {
                self.last_change = Some(now);
                self.frames.clear();
                return Some(Step::Up);
            }
        }
        None
    }

    /// Forgets the frames seen (a level set by hand, a place opened).
    pub fn reset(&mut self) {
        self.frames.clear();
        self.last_change = None;
        self.hold = None;
    }
}

fn median(v: &[f32]) -> f32 {
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_within_range_and_ordered() {
        let mut prev: Option<RenderSettings> = None;
        for level in 1..=LEVELS {
            let s = RenderSettings::level(level);
            assert_eq!(s, s.normalized(), "{level}");
            assert!(matches!(s.msaa, 1 | 2 | 4));
            assert!(s.sun_cascades as usize <= crate::sun_shadow::CASCADES);
            if let Some(p) = prev {
                assert!(s.view_distance >= p.view_distance, "{level}");
                assert!(s.detail_px <= p.detail_px, "{level}");
                assert!(s.texture_texels >= p.texture_texels, "{level}");
                assert!(s.render_scale >= p.render_scale, "{level}");
                assert!(s.msaa >= p.msaa, "{level}");
            }
            prev = Some(s);
        }
        // The top level is Studio's defaults: everything, nothing culled.
        let top = RenderSettings::level(LEVELS);
        assert_eq!(top.msaa, 4);
        assert!(top.hdr && top.ssao && top.dof && top.clouds && top.env_probe && top.light_shadows);
        assert!(top.view_distance.is_infinite());
        assert_eq!(top.detail_px, mech::LOD_MIN_PX);
        assert_eq!(top.texture_texels, crate::matpack::LAYER_SIZE);
        let bottom = RenderSettings::level(1);
        assert_eq!(bottom.msaa, 1);
        assert!(!bottom.hdr);
    }

    #[test]
    fn build_key_tells_rebuilds_from_frame_switches() {
        let a = RenderSettings::level(LEVELS);
        let mut b = a;
        b.ssao = false;
        b.bloom_levels = 0;
        b.render_scale = 0.5;
        b.view_distance = 500.0;
        assert_eq!(a.build_key(), b.build_key());
        b.msaa = 1;
        assert_ne!(a.build_key(), b.build_key());
    }

    #[test]
    fn overrides_sit_over_the_level() {
        let o = Overrides { shadows: Some(Shadows::Off), texture_texels: Some(256), bloom: Some(true), ..Default::default() };
        let s = RenderSettings::level(LEVELS).with_overrides(&o);
        assert_eq!(s.sun_cascades, 0);
        assert_eq!(s.texture_texels, 256);
        assert_eq!(s.bloom_levels, 5);
        let s = RenderSettings::level(1).with_overrides(&Overrides { bloom: Some(true), ..Default::default() });
        assert_eq!(s.bloom_levels, 3);
        assert!(!Overrides::default().any());
        assert!(o.any());
    }

    #[test]
    fn studio_levels_round_trip() {
        for level in 1..=LEVELS {
            assert_eq!(level_from_studio(level_to_studio(level)), Some(level), "{level}");
        }
        assert_eq!(level_to_studio(LEVELS), 21);
        assert_eq!(level_from_studio(0), None);
        assert_eq!(level_from_studio(21), Some(LEVELS));
        assert_eq!(level_from_studio(1), Some(1));
    }

    #[test]
    fn names_parse() {
        assert_eq!(parse_level("auto"), Some(None));
        assert_eq!(parse_level("LOW"), Some(Some(3)));
        assert_eq!(parse_level("high"), Some(Some(LEVELS)));
        assert_eq!(parse_level("7"), Some(Some(7)));
        assert_eq!(parse_level("11"), None);
        assert_eq!(parse_level("ultra"), None);
    }

    #[test]
    fn automatic_steps_down_on_slow_frames_and_holds() {
        let mut auto = Auto::default();
        let t0 = Instant::now();
        let mut now = t0;
        let mut level = 5;
        let mut steps = Vec::new();
        // Three seconds of 30 ms frames: one step down, not two.
        for _ in 0..100 {
            now += Duration::from_millis(30);
            if let Some(s) = auto.frame(now, 0.030, level) {
                steps.push(s);
                level -= 1;
            }
        }
        assert_eq!(steps, vec![Step::Down]);
        assert_eq!(level, 4);
        // Fast frames for the up time: no step up while the hold lasts
        // (and no step down from the slow frames still in the window).
        for _ in 0..(25 * 60) {
            now += Duration::from_micros(16_700);
            if let Some(s) = auto.frame(now, 0.0167, level) {
                steps.push(s);
            }
        }
        assert_eq!(steps, vec![Step::Down]);
        // Past the hold, the fast frames take it back up.
        now += Duration::from_secs_f32(mech::QUALITY_AUTO_HOLD_S);
        for _ in 0..(25 * 60) {
            now += Duration::from_micros(16_700);
            if let Some(s) = auto.frame(now, 0.0167, level) {
                steps.push(s);
                level += 1;
            }
        }
        assert_eq!(steps, vec![Step::Down, Step::Up]);
        assert_eq!(level, 5);
    }

    #[test]
    fn automatic_ignores_idle_pauses() {
        let mut auto = Auto::default();
        let mut now = Instant::now();
        for _ in 0..100 {
            now += Duration::from_millis(500);
            assert_eq!(auto.frame(now, 0.5, 5), None);
        }
    }
}

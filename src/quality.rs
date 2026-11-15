//! Render quality: one struct ([`RenderSettings`]) says what the 3D view
//! spends its GPU on, and a preset per [`Quality`] fills it in. The
//! renderer (render.rs) makes only the pipelines and render targets the
//! settings call for and skips the passes they turn off; the viewport
//! sizes its target by `render_scale`.
//!
//! A phone starts at Low, a computer at High (`platform_default`). View >
//! Quality picks another preset; View > Render Scale and View > Ambient
//! Occlusion change single fields, which the layout store keeps across
//! runs (layout_store.rs). `RUSTUDIO_QUALITY=low|medium|high` and the
//! per-field `RUSTUDIO_*` switches below override for a run (on Android
//! through `adb shell setprop debug.rustudio.env`, docs/ANDROID.md).
//!
//! What each field costs on a phone's GPU (a tiler with a tenth of a
//! desktop card's bandwidth): MSAA multiplies the colour and depth kept
//! per pixel in tile memory (4× plus a 64-bit HDR colour leaves a
//! Adreno's tiles a few thousand pixels, so the scene is binned and
//! drawn many times over); the 64-bit HDR target doubles every
//! full-screen read and write against 32-bit RG11B10 float; bloom, SSAO
//! and depth of field are each three to fourteen full- or half-screen
//! passes; each sun cascade and each shadowed light is a pass over the
//! casters plus blurs.

use egui_wgpu::wgpu;

/// A preset: what a class of device can draw at its refresh rate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Quality {
    /// Phones: no MSAA, 32-bit float scene, 70 % render scale, one sun
    /// cascade at half the texels, no light shadows / SSAO / bloom / DoF
    /// / clouds / environment probe.
    Low,
    /// Laptops with an integrated GPU: 2× MSAA, HDR, two cascades,
    /// bloom with a shorter pyramid, no SSAO / DoF.
    Medium,
    /// Everything, as Studio's highest quality levels draw it.
    High,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Low, Quality::Medium, Quality::High];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
        }
    }

    pub fn parse(s: &str) -> Option<Quality> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" | "l" | "0" => Some(Quality::Low),
            "medium" | "med" | "m" | "1" => Some(Quality::Medium),
            "high" | "h" | "2" => Some(Quality::High),
            _ => None,
        }
    }

    /// Low on Android, High elsewhere; `RUSTUDIO_QUALITY` overrides.
    pub fn platform_default() -> Quality {
        let default = if cfg!(target_os = "android") { Quality::Low } else { Quality::High };
        std::env::var("RUSTUDIO_QUALITY").ok().and_then(|v| Quality::parse(&v)).unwrap_or(default)
    }
}

/// What the renderer draws and with what. Fields marked (rebuild) change
/// the pipelines or the render targets: the renderer remakes them when
/// one changes (`Renderer::set_settings`); the others take effect on the
/// next frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RenderSettings {
    /// The preset these started from (View > Quality shows it).
    pub quality: Quality,
    /// Scene multisampling: 1 (off), 2 or 4 (rebuild).
    pub msaa: u32,
    /// The scene in 64-bit Rgba16Float (true) or 32-bit Rg11b10Ufloat
    /// where the device renders to it, else Rgba16Float (rebuild). Both
    /// hold linear light above 1: exposure and the tone curve stay in
    /// the post pass.
    pub hdr: bool,
    /// The 3D view's pixels per panel pixel, 0.25 ..= 1 (View > Render
    /// Scale; the viewport sizes the target by it).
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
    /// Screen-space ambient occlusion (View > Ambient Occlusion).
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
    pub fn preset(quality: Quality) -> Self {
        match quality {
            Quality::High => RenderSettings {
                quality,
                msaa: 4,
                hdr: true,
                render_scale: 1.0,
                sun_cascades: crate::sun_shadow::CASCADES as u32,
                shadow_scale: 1.0,
                light_shadows: true,
                ssao: true,
                bloom_levels: 5,
                dof: true,
                clouds: true,
                env_probe: true,
            },
            Quality::Medium => RenderSettings {
                quality,
                msaa: 2,
                hdr: true,
                render_scale: 1.0,
                sun_cascades: 2,
                shadow_scale: 0.75,
                light_shadows: true,
                ssao: false,
                bloom_levels: 3,
                dof: false,
                clouds: true,
                env_probe: true,
            },
            Quality::Low => RenderSettings {
                quality,
                msaa: 1,
                hdr: false,
                render_scale: crate::mech::RENDER_SCALE_LOW,
                sun_cascades: 1,
                shadow_scale: crate::mech::SUN_SHADOW_TEXEL_SCALE_PHONE,
                light_shadows: false,
                ssao: false,
                bloom_levels: 0,
                dof: false,
                clouds: false,
                env_probe: false,
            },
        }
    }

    /// The platform's preset with the run's environment applied.
    pub fn platform_default() -> Self {
        Self::preset(Quality::platform_default()).with_env()
    }

    /// The `RUSTUDIO_*` switches over these settings: `RUSTUDIO_MSAA`
    /// (1, 2, 4), `RUSTUDIO_HDR` (0|1), `RUSTUDIO_RENDER_SCALE` (0.25 to
    /// 1), `RUSTUDIO_SUN_SHADOWS` (0: no cascades), `RUSTUDIO_SUN_CASCADES`
    /// (0 to 4), `RUSTUDIO_SHADOW_SCALE` (0.25 to 1), `RUSTUDIO_LIGHT_SHADOWS`
    /// (0|1), `RUSTUDIO_SSAO` (0|1), `RUSTUDIO_BLOOM` (levels, 0 off),
    /// `RUSTUDIO_DOF` (0|1), `RUSTUDIO_CLOUDS` (0|1), `RUSTUDIO_NO_ENV_PROBE`.
    pub fn with_env(mut self) -> Self {
        let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let flag = |k: &str| var(k).map(|v| v != "0" && !v.eq_ignore_ascii_case("off") && !v.eq_ignore_ascii_case("false"));
        let num = |k: &str| var(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
        if let Some(n) = num("RUSTUDIO_MSAA") {
            self.msaa = match n as u32 {
                0 | 1 => 1,
                2 | 3 => 2,
                _ => 4,
            };
        }
        if let Some(on) = flag("RUSTUDIO_HDR") {
            self.hdr = on;
        }
        if let Some(s) = num("RUSTUDIO_RENDER_SCALE") {
            self.render_scale = s.clamp(0.25, 1.0);
        }
        if flag("RUSTUDIO_SUN_SHADOWS") == Some(false) {
            self.sun_cascades = 0;
        }
        if let Some(n) = num("RUSTUDIO_SUN_CASCADES") {
            self.sun_cascades = (n.max(0.0) as u32).min(crate::sun_shadow::CASCADES as u32);
        }
        if let Some(s) = num("RUSTUDIO_SHADOW_SCALE") {
            self.shadow_scale = s.clamp(0.25, 1.0);
        }
        if let Some(on) = flag("RUSTUDIO_LIGHT_SHADOWS") {
            self.light_shadows = on;
        }
        if let Some(on) = flag("RUSTUDIO_SSAO") {
            self.ssao = on;
        }
        if let Some(n) = num("RUSTUDIO_BLOOM") {
            self.bloom_levels = (n.max(0.0) as u32).min(5);
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
        self.normalized()
    }

    /// Fields within their ranges.
    pub fn normalized(mut self) -> Self {
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
            "quality {}: MSAA {}, scene {}, render scale {:.2}, sun cascades {} at x{:.2}, light shadows {}, AO {}, bloom {}, DoF {}, clouds {}, env probe {}",
            self.quality.name().to_ascii_lowercase(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_within_range() {
        for q in Quality::ALL {
            let s = RenderSettings::preset(q);
            assert_eq!(s, s.normalized(), "{q:?}");
            assert!(matches!(s.msaa, 1 | 2 | 4));
            assert!(s.sun_cascades as usize <= crate::sun_shadow::CASCADES);
        }
        assert_eq!(RenderSettings::preset(Quality::Low).msaa, 1);
        assert!(!RenderSettings::preset(Quality::Low).hdr);
        assert_eq!(RenderSettings::preset(Quality::High).msaa, 4);
    }

    #[test]
    fn build_key_tells_rebuilds_from_frame_switches() {
        let a = RenderSettings::preset(Quality::High);
        let mut b = a;
        b.ssao = false;
        b.bloom_levels = 0;
        b.render_scale = 0.5;
        assert_eq!(a.build_key(), b.build_key());
        b.msaa = 1;
        assert_ne!(a.build_key(), b.build_key());
    }

    #[test]
    fn names_parse() {
        for q in Quality::ALL {
            assert_eq!(Quality::parse(q.name()), Some(q));
        }
        assert_eq!(Quality::parse("LOW"), Some(Quality::Low));
        assert_eq!(Quality::parse("ultra"), None);
    }
}

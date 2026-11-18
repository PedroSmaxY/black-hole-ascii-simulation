//! Relativistic Accretion Disk Shader.
//! Combines Keplerian differential rotation, seamless polar plasma turbulence,
//! blackbody thermal emission, gravitational redshift, and Doppler beaming.

use crate::math::Vec3;
use crate::physics::black_hole::{BlackHole, DiskCrossing, DopplerMode};

pub struct DiskShader;

impl DiskShader {
    /// Compute relativistic frequency shift factor `g = nu_obs / nu_emit`.
    /// `g > 1` when plasma moves toward the observer (blueshift + beaming),
    /// `g < 1` when plasma recedes (redshift + dimming).
    #[inline(always)]
    pub fn doppler_factor(bh: &BlackHole, pos: Vec3, ray_dir: Vec3, r: f64) -> f64 {
        let beta = bh.orbital_beta(r);
        let gamma_inv = (1.0 - beta * beta).max(1e-4).sqrt();
        let grav_redshift = bh.time_dilation(r).max(0.05);

        // Prograde tangent direction at (x, 0, z)
        let tangent = Vec3::new(-pos.z, 0.0, pos.x).normalize() * bh.spin_dir;
        // Photon travels from disk toward camera along -ray_dir
        let to_observer = -ray_dir.normalize();
        let cos_alpha = tangent.dot(to_observer).clamp(-1.0, 1.0);

        (grav_redshift * gamma_inv) / (1.0 - beta * cos_alpha).max(0.1)
    }

    /// Seamless multi-octave polar turbulence with Keplerian shear.
    #[inline(always)]
    fn plasma_turbulence(r: f64, phi: f64, bh: &BlackHole, elapsed: f64) -> f64 {
        let omega = bh.kepler_omega(r);
        let sheared_phi = phi - omega * elapsed * bh.spin_dir;

        // Use (cos, sin) to guarantee zero seam at phi = +-PI
        let c1 = sheared_phi.cos();
        let s1 = sheared_phi.sin();
        let c2 = (2.0 * sheared_phi + r * 0.65).cos();
        let s2 = (2.0 * sheared_phi + r * 0.65).sin();

        // Concentric fine rings + spiral filaments
        let rings = (r * 3.8 + c1 * 0.9 + s2 * 0.45).sin() * 0.5 + 0.5;
        let fine_rings = (r * 11.5 - s1 * 1.4).cos() * 0.5 + 0.5;
        let spiral = (3.0 * sheared_phi + r * 1.35 + c2 * 0.8).sin() * 0.5 + 0.5;
        let micro = (5.0 * sheared_phi - r * 4.2 + s2 * 1.2).cos() * 0.5 + 0.5;

        (0.42 * rings + 0.28 * spiral + 0.18 * fine_rings + 0.12 * micro).clamp(0.0, 1.0)
    }

    /// Approximate blackbody temperature (Kelvin) -> linear HDR RGB.
    #[inline(always)]
    fn blackbody_rgb(temp_k: f64) -> [f32; 3] {
        let t = (temp_k / 100.0).clamp(10.0, 220.0);

        let r = if t <= 66.0 {
            1.0
        } else {
            let x = t - 60.0;
            (1.292936 * x.powf(-0.13320475)).clamp(0.0, 1.0)
        };

        let g = if t <= 66.0 {
            (0.39008157 * t.ln() - 0.63184144).clamp(0.0, 1.0)
        } else {
            let x = t - 60.0;
            (1.12989086 * x.powf(-0.07551484)).clamp(0.0, 1.0)
        };

        let b = if t >= 66.0 {
            1.0
        } else if t <= 19.0 {
            0.0
        } else {
            let x = t - 10.0;
            (0.54320678 * x.ln() - 1.19625408).clamp(0.0, 1.0)
        };

        // Convert sRGB-like curve to linear for HDR composition
        [(r as f32).powf(2.2), (g as f32).powf(2.2), (b as f32).powf(2.2)]
    }

    /// Evaluate linear HDR emission RGB and opacity [0..1] for a disk crossing.
    pub fn sample(bh: &BlackHole, hit: DiskCrossing, elapsed: f64) -> ([f32; 3], f32) {
        let r = hit.r;
        let t_norm = ((r - bh.disk_inner) / (bh.disk_outer - bh.disk_inner)).clamp(0.0, 1.0);

        // Smooth inner edge (plunging region) & outer fade
        let inner_fade = ((r - bh.disk_inner) / 0.55).clamp(0.0, 1.0);
        let inner_fade = inner_fade * inner_fade * (3.0 - 2.0 * inner_fade);
        let outer_fade = (1.0 - t_norm).powf(1.8);

        let turb = Self::plasma_turbulence(r, hit.phi, bh, elapsed);
        let density = inner_fade * outer_fade * (0.38 + 0.82 * turb) * bh.mass_scale.clamp(0.0, 1.0);
        if density <= 0.002 {
            return ([0.0, 0.0, 0.0], 0.0);
        }

        // Base temperature profile T(r) ~ r^(-3/4), hotter near ISCO
        let base_temp_k = 1900.0 + 4800.0 * (bh.disk_inner / r).powf(0.85);
        let g = Self::doppler_factor(bh, hit.pos, hit.dir, r);

        let (eff_temp, beaming, tint) = match bh.doppler_mode {
            DopplerMode::Cinematic => {
                // Softened Doppler beaming with warm golden-white thermal palette
                let g_soft = 1.0 + (g - 1.0) * 0.36;
                let beam = g_soft.clamp(0.35, 2.2).powf(2.2);
                let temp = base_temp_k * (0.85 + 0.30 * g_soft);
                (temp, beam, [1.05_f32, 0.94_f32, 0.80_f32])
            }
            DopplerMode::Physical => {
                // Full relativistic Doppler beaming & frequency shift
                let beam = g.clamp(0.18, 3.2).powf(3.5);
                let temp = base_temp_k * g;
                (temp, beam, [1.0_f32, 1.0_f32, 1.0_f32])
            }
        };

        let base_rgb = Self::blackbody_rgb(eff_temp);
        // Inner photon-ring / ISCO energy boost for HDR bloom
        let radial_boost = 1.0 + 3.2 * (bh.disk_inner / r).powf(2.4);

        // Relativistic orbiting hot-spots (Sgr A* flares) near inner disk
        let hs_r = bh.disk_inner * 1.32;
        let hs_omega = bh.kepler_omega(hs_r);
        let hs_phi1 = (hs_omega * elapsed * bh.spin_dir).rem_euclid(2.0 * std::f64::consts::PI);
        let hs_phi2 = (hs_omega * elapsed * bh.spin_dir + 2.6).rem_euclid(2.0 * std::f64::consts::PI);
        let dphi1 = (hit.phi - hs_phi1).sin().abs();
        let dphi2 = (hit.phi - hs_phi2).sin().abs();
        let dr = (r - hs_r).abs();
        let hotspot = ((-dr * dr * 2.2 - dphi1 * dphi1 * 18.0).exp()
            + 0.7 * (-dr * dr * 1.8 - dphi2 * dphi2 * 14.0).exp())
            * 1.45;

        let intensity = (density * beaming * (radial_boost + hotspot) * (0.65 + 0.7 * turb)) as f32;

        let hdr = [
            base_rgb[0] * tint[0] * intensity,
            base_rgb[1] * tint[1] * intensity,
            base_rgb[2] * tint[2] * intensity,
        ];
        let alpha = (density * 1.15).clamp(0.0, 0.94) as f32;
        (hdr, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doppler_asymmetry() {
        let bh = BlackHole::new();
        // Camera at +Z looking toward origin (-Z).
        // At (+x, 0, 0) with spin_dir=+1, tangent is (0, 0, +1) -> moving TOWARD camera (+Z).
        let pos_approaching = Vec3::new(5.0, 0.0, 0.0);
        let pos_receding = Vec3::new(-5.0, 0.0, 0.0);
        let ray_dir = Vec3::new(0.0, 0.0, -1.0);

        let g_app = DiskShader::doppler_factor(&bh, pos_approaching, ray_dir, 5.0);
        let g_rec = DiskShader::doppler_factor(&bh, pos_receding, ray_dir, 5.0);

        assert!(g_app > g_rec, "Approaching side must have higher Doppler factor than receding side");
        assert!(g_app > 1.0, "Approaching side at r=5 should be blueshifted (g > 1)");
        assert!(g_rec < 1.0, "Receding side at r=5 should be redshifted (g < 1)");
    }
}

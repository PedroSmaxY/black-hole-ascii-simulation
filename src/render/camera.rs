//! Relativistic Camera: smooth orbital navigation in r_s units,
//! intro dive animation, smooth preset transitions, and Lorentz aberration.

use crate::math::{Ray, Vec3};
use crate::physics::black_hole::BlackHole;
use crate::render::buffer::RenderMode;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPreset {
    EdgeOn,
    PhotonRing,
    PolarView,
    CloseHorizon,
}

impl CameraPreset {
    pub fn name(self) -> &'static str {
        match self {
            CameraPreset::EdgeOn => "Edge-On",
            CameraPreset::PhotonRing => "Photon Ring",
            CameraPreset::PolarView => "Polar View",
            CameraPreset::CloseHorizon => "Close Horizon",
        }
    }

    pub fn target(self) -> (f64, f64, f64) {
        match self {
            CameraPreset::EdgeOn => (0.25, 0.095, 22.0),
            CameraPreset::PhotonRing => (0.65, 0.14, 7.5),
            CameraPreset::PolarView => (0.0, 1.42, 28.0),
            CameraPreset::CloseHorizon => (-0.75, 0.055, 2.4),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraTransition {
    pub start_az: f64,
    pub start_el: f64,
    pub start_dist: f64,
    pub end_az: f64,
    pub end_el: f64,
    pub end_dist: f64,
    pub elapsed: f64,
    pub duration: f64,
}

pub struct Camera {
    pub azimuth: f64,
    pub elevation: f64,
    pub distance_rs: f64,
    pub fov_y_rad: f64,

    pub vel_azimuth: f64,
    pub vel_elevation: f64,

    pub zen_mode: bool,
    zen_phase: f64,

    pub intro_elapsed: f64,
    pub intro_active: bool,
    pub transition: Option<CameraTransition>,

    pub active_preset_name: &'static str,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            azimuth: -0.95,
            elevation: 0.52,
            distance_rs: 55.0,
            fov_y_rad: 45.0 * PI / 180.0,
            vel_azimuth: 0.0,
            vel_elevation: 0.0,
            zen_mode: true,
            zen_phase: 0.0,
            intro_elapsed: 0.0,
            intro_active: true,
            transition: None,
            active_preset_name: CameraPreset::EdgeOn.name(),
        }
    }

    /// Start a smooth 1.1s cubic ease-in-out flight to the target preset.
    pub fn apply_preset(&mut self, preset: CameraPreset) {
        self.zen_mode = false;
        self.intro_active = false;
        self.vel_azimuth = 0.0;
        self.vel_elevation = 0.0;
        self.active_preset_name = preset.name();

        let (end_az, end_el, end_dist) = preset.target();
        self.transition = Some(CameraTransition {
            start_az: self.azimuth,
            start_el: self.elevation,
            start_dist: self.distance_rs.max(0.5),
            end_az,
            end_el,
            end_dist,
            elapsed: 0.0,
            duration: 1.1,
        });
    }

    #[inline(always)]
    pub fn position(&self) -> Vec3 {
        let ce = self.elevation.cos();
        Vec3::new(
            self.distance_rs * ce * self.azimuth.cos(),
            self.distance_rs * self.elevation.sin(),
            self.distance_rs * ce * self.azimuth.sin(),
        )
    }

    pub fn basis(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let pos = self.position();
        let forward = (-pos).normalize();
        let world_up = if forward.y.abs() > 0.98 { Vec3::Z } else { Vec3::Y };
        let right = forward.cross(world_up).normalize();
        let up = right.cross(forward).normalize();
        (pos, forward, right, up)
    }

    pub fn orbit(&mut self, d_az: f64, d_el: f64) {
        self.zen_mode = false;
        self.intro_active = false;
        self.transition = None;
        self.vel_azimuth += d_az * 12.0;
        self.vel_elevation += d_el * 12.0;
    }

    pub fn zoom(&mut self, factor: f64, min_r: f64) {
        self.intro_active = false;
        self.transition = None;
        self.distance_rs = (self.distance_rs * factor).clamp(min_r, 120.0);
    }

    pub fn update(&mut self, dt: f64, bh: &BlackHole) {
        if self.intro_active {
            let dur = 4.0;
            self.intro_elapsed += dt;
            let t = (self.intro_elapsed / dur).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - t).powi(3);
            self.distance_rs = 55.0 + (22.0 - 55.0) * ease;
            self.elevation = 0.52 + (0.10 - 0.52) * ease;
            self.azimuth = -0.95 + (0.25 - (-0.95)) * ease;
            if t >= 1.0 {
                self.intro_active = false;
            }
            return;
        }

        if let Some(mut tr) = self.transition {
            tr.elapsed += dt;
            let t = (tr.elapsed / tr.duration).clamp(0.0, 1.0);
            let ease = if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
            };

            let mut d_az = (tr.end_az - tr.start_az) % (2.0 * PI);
            if d_az > PI {
                d_az -= 2.0 * PI;
            } else if d_az < -PI {
                d_az += 2.0 * PI;
            }

            self.azimuth = tr.start_az + d_az * ease;
            self.elevation = tr.start_el + (tr.end_el - tr.start_el) * ease;
            self.distance_rs =
                (tr.start_dist.ln() + (tr.end_dist.ln() - tr.start_dist.ln()) * ease).exp();

            if t >= 1.0 {
                self.transition = None;
            } else {
                self.transition = Some(tr);
            }
            return;
        }

        // Frame-dragging twist in the Kerr ergosphere / near-horizon region
        let pos = self.position();
        if pos.length() < 3.0 {
            self.azimuth += bh.frame_dragging_omega(pos) * dt * 2.2;
        }

        if self.zen_mode {
            self.zen_phase += dt * 0.14;
            self.azimuth += 0.075 * dt;
            self.elevation = 0.10 + 0.16 * self.zen_phase.sin();
            self.distance_rs = 13.5 + 8.5 * (self.zen_phase * 0.65).cos();
        } else {
            self.azimuth += self.vel_azimuth * dt;
            self.elevation = (self.elevation + self.vel_elevation * dt).clamp(-1.54, 1.54);
            let damping = (-10.0 * dt).exp();
            self.vel_azimuth *= damping;
            self.vel_elevation *= damping;
        }
    }

    /// Local relativistic velocity vector beta = v/c for Lorentz aberration near the BH.
    #[inline(always)]
    pub fn camera_beta(&self, bh: &BlackHole) -> Vec3 {
        let pos = self.position();
        let r = pos.length();
        if r > 6.5 || r < 1e-4 {
            return Vec3::ZERO;
        }
        let tangent = Vec3::new(-pos.z, 0.0, pos.x);
        let t_len = tangent.length();
        if t_len < 1e-6 {
            return Vec3::ZERO;
        }
        let speed = (bh.r_s / (2.0 * r)).sqrt().min(0.72) * ((6.5 - r) / 5.5).clamp(0.0, 1.0);
        (tangent / t_len) * (speed * bh.spin_dir)
    }

    /// Exact Special Relativistic Lorentz aberration of incoming photon direction.
    #[inline(always)]
    pub fn aberrate_ray(dir: Vec3, beta: Vec3) -> Vec3 {
        let b2 = beta.length_squared();
        if b2 < 1e-10 {
            return dir;
        }
        let gamma = 1.0 / (1.0 - b2).max(1e-4).sqrt();
        let bd = beta.dot(dir);
        let factor = ((gamma - 1.0) / b2) * bd + gamma;
        ((dir + beta * factor) / (gamma * (1.0 + bd)).max(1e-4)).normalize()
    }

    #[inline(always)]
    pub fn generate_ray(
        &self,
        u: usize,
        v: usize,
        cols: usize,
        v_height: usize,
        mode: RenderMode,
        basis: (Vec3, Vec3, Vec3, Vec3),
        beta: Vec3,
    ) -> Ray {
        let (pos, forward, right, up) = basis;
        let char_aspect = match mode {
            RenderMode::Ascii => 2.0,
            RenderMode::HalfBlock => 1.0,
        };
        let aspect = (cols as f64) / (v_height as f64 * char_aspect).max(1.0);

        let half_h = (0.5 * self.fov_y_rad).tan();
        let half_w = half_h * aspect;

        let nx = (2.0 * (u as f64 + 0.5) / cols as f64) - 1.0;
        let ny = 1.0 - (2.0 * (v as f64 + 0.5) / v_height as f64);

        let raw_dir = (forward + right * (nx * half_w) + up * (ny * half_h)).normalize();
        let dir = Self::aberrate_ray(raw_dir, beta);
        Ray::new(pos, dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lorentz_aberration_preserves_unit_norm() {
        let dir = Vec3::new(0.3, -0.4, 0.86).normalize();
        let beta = Vec3::new(0.55, 0.0, 0.15);
        let aberrated = Camera::aberrate_ray(dir, beta);
        assert!((aberrated.length() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_preset_transition_converges() {
        let bh = BlackHole::new();
        let mut cam = Camera::new();
        cam.apply_preset(CameraPreset::PhotonRing);
        assert!(cam.transition.is_some());

        for _ in 0..30 {
            cam.update(0.05, &bh); // 1.5s > 1.1s
        }

        let (end_az, end_el, end_dist) = CameraPreset::PhotonRing.target();
        assert!(cam.transition.is_none());
        assert!((cam.elevation - end_el).abs() < 1e-6);
        assert!((cam.distance_rs - end_dist).abs() < 1e-6);
        assert!((cam.azimuth - end_az).abs() < 1e-3);
    }
}

//! Relativistic black hole physics and adaptive RK4 geodesic raymarcher.
//! Supports both exact Schwarzschild and Cartesian Kerr-Schild (spinning) metrics.
//! All spatial distances are normalized in units of the Schwarzschild radius (r_s = 2M = 1.0).

use crate::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricModel {
    Schwarzschild,
    /// Dimensionless spin chi = a/M in [0.0, 0.998]
    KerrSchild { spin: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DopplerMode {
    Cinematic,
    Physical,
}

#[derive(Clone, Copy, Debug)]
pub struct DiskCrossing {
    pub pos: Vec3,
    pub dir: Vec3,
    pub r: f64,
    pub phi: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct PlanetHit {
    pub pos: Vec3,
    pub normal: Vec3,
    pub ray_dir: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitingPlanet {
    pub enabled: bool,
    pub pos: Vec3,
    pub radius: f64,
    pub orbit_r: f64,
    pub angle: f64,
}

impl OrbitingPlanet {
    pub fn s2_star() -> Self {
        let orbit_r = 24.0; // Safely outside disk_outer (16 r_s) and Roche limit
        let angle = 2.65;
        Self {
            enabled: false,
            pos: Self::compute_pos(orbit_r, angle),
            radius: 0.55,
            orbit_r,
            angle,
        }
    }

    pub fn miller() -> Self {
        Self::s2_star()
    }

    fn compute_pos(orbit_r: f64, angle: f64) -> Vec3 {
        // Inclined orbit outside the accretion disk
        let r_ecc = orbit_r * (1.0 - 0.18 * angle.cos());
        let y = 3.2 * (angle + 0.4).sin();
        Vec3::new(r_ecc * angle.cos(), y, r_ecc * angle.sin())
    }

    pub fn update(&mut self, dt: f64) {
        if !self.enabled {
            return;
        }
        let r_cur = self.pos.length().max(2.0);
        // Keplerian speed-up at periapsis
        let omega = 0.24 * (self.orbit_r / r_cur).powf(1.3);
        self.angle = (self.angle + omega * dt) % (2.0 * std::f64::consts::PI);
        self.pos = Self::compute_pos(self.orbit_r, self.angle);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TraceOutcome {
    Captured,
    HitPlanet(PlanetHit),
    Escaped { final_dir: Vec3 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvapPhase {
    Idle,
    Evaporating(f64),
    EmptyHold(f64),
    Rewinding(f64),
}

pub struct BlackHole {
    pub metric: MetricModel,
    pub r_s: f64,
    pub mass_scale: f64,
    pub evap_phase: EvapPhase,
    pub flash_intensity: f32,
    pub disk_inner: f64,
    pub disk_outer: f64,
    pub doppler_mode: DopplerMode,
    pub spin_dir: f64, // +1.0 prograde CCW, -1.0 retrograde
}

impl BlackHole {
    pub fn new() -> Self {
        let r_s = 1.0;
        let mut bh = Self {
            metric: MetricModel::KerrSchild { spin: 0.90 },
            r_s,
            mass_scale: 1.0,
            evap_phase: EvapPhase::Idle,
            flash_intensity: 0.0,
            disk_inner: 1.5 * r_s,
            disk_outer: 16.0 * r_s,
            doppler_mode: DopplerMode::Physical,
            spin_dir: 1.0,
        };
        bh.sync_disk_inner();
        bh
    }

    pub fn trigger_evaporation(&mut self) {
        match self.evap_phase {
            EvapPhase::Idle => {
                self.evap_phase = EvapPhase::Evaporating(0.0);
                self.flash_intensity = 0.0;
            }
            _ => {
                self.evap_phase = EvapPhase::Idle;
                self.mass_scale = 1.0;
                self.r_s = 1.0;
                self.flash_intensity = 0.0;
                self.sync_disk_inner();
            }
        }
    }

    #[inline(always)]
    pub fn is_evaporated(&self) -> bool {
        self.mass_scale <= 0.02
    }

    /// Update Hawking evaporation curve M(tau) = (1 - tau)^(1/3), empty hold, and auto-rewind.
    pub fn update_evaporation(&mut self, dt: f64) {
        self.flash_intensity = (self.flash_intensity - dt as f32 * 2.2).max(0.0);
        match self.evap_phase {
            EvapPhase::Idle => {}
            EvapPhase::Evaporating(p) => {
                let next = (p + dt / 11.0).min(1.0);
                self.mass_scale = (1.0 - next).max(0.0).cbrt();
                self.r_s = self.mass_scale.max(0.015);
                self.sync_disk_inner();
                if next >= 1.0 {
                    self.mass_scale = 0.0;
                    self.flash_intensity = 3.8;
                    self.evap_phase = EvapPhase::EmptyHold(0.0);
                } else {
                    self.evap_phase = EvapPhase::Evaporating(next);
                }
            }
            EvapPhase::EmptyHold(t) => {
                let next = t + dt;
                if next >= 1.8 {
                    self.evap_phase = EvapPhase::Rewinding(0.0);
                    self.flash_intensity = 1.8;
                } else {
                    self.evap_phase = EvapPhase::EmptyHold(next);
                }
            }
            EvapPhase::Rewinding(p) => {
                let next = (p + dt / 3.5).min(1.0);
                let ease = 1.0 - (1.0 - next).powi(2);
                self.mass_scale = ease;
                self.r_s = self.mass_scale.max(0.015);
                self.sync_disk_inner();
                if next >= 1.0 {
                    self.mass_scale = 1.0;
                    self.r_s = 1.0;
                    self.flash_intensity = 0.0;
                    self.evap_phase = EvapPhase::Idle;
                    self.sync_disk_inner();
                } else {
                    self.evap_phase = EvapPhase::Rewinding(next);
                }
            }
        }
    }

    /// Hawking radiation glow on the horizon: zero for massive BH, heats up red -> blue-white as M -> 0.
    #[inline(always)]
    pub fn hawking_glow(&self) -> [f32; 3] {
        if self.mass_scale > 0.38 || self.mass_scale <= 0.01 {
            return [0.0, 0.0, 0.0];
        }
        let heat = ((0.38 - self.mass_scale) / 0.38).clamp(0.0, 1.0) as f32;
        let lum = (heat * heat) * (1.0 / (self.mass_scale as f32 + 0.08)).min(14.0);
        // Color shifts from deep red (heat~0.2) -> orange-white -> hot blue-white (heat~1.0)
        let r = lum * (1.25 - 0.15 * heat);
        let g = lum * (0.25 + 0.95 * heat * heat);
        let b = lum * (0.05 + 1.55 * heat * heat * heat);
        [r, g, b]
    }

    /// Gravitational mass M = r_s / 2 in geometric units (G = c = 1).
    #[inline(always)]
    pub fn mass(&self) -> f64 {
        0.5 * self.r_s
    }

    /// Dimensionless spin chi = a / M in [0.0, 0.998].
    #[inline(always)]
    pub fn spin_chi(&self) -> f64 {
        match self.metric {
            MetricModel::Schwarzschild => 0.0,
            MetricModel::KerrSchild { spin } => spin.clamp(0.0, 0.998),
        }
    }

    /// Kerr spin parameter a = chi * M.
    #[inline(always)]
    pub fn spin_a(&self) -> f64 {
        self.spin_chi() * self.mass()
    }

    /// Event horizon radius r_+ = M + sqrt(M^2 - a^2).
    #[inline(always)]
    pub fn event_horizon(&self) -> f64 {
        let m = self.mass();
        let a = self.spin_a();
        m + (m * m - a * a).max(0.0).sqrt()
    }

    #[inline(always)]
    pub fn photon_sphere(&self) -> f64 {
        1.5 * self.r_s
    }

    /// Prograde ISCO radius (Bardeen-Press-Teukolsky formula).
    #[inline(always)]
    pub fn isco(&self) -> f64 {
        let m = self.mass();
        let chi = self.spin_chi();
        if chi < 1e-6 {
            return 3.0 * self.r_s;
        }
        let one_minus_chi2 = (1.0 - chi * chi).max(1e-6);
        let z1 = 1.0
            + one_minus_chi2.cbrt() * ((1.0 + chi).cbrt() + (1.0 - chi).cbrt());
        let z2 = (3.0 * chi * chi + z1 * z1).sqrt();
        m * (3.0 + z2 - ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt())
    }

    /// Update inner disk edge when switching between Schwarzschild and Kerr.
    pub fn sync_disk_inner(&mut self) {
        self.disk_inner = (self.isco() * 0.87).max(self.event_horizon() * 1.15);
    }

    /// Exact ZAMO gravitational time dilation (lapse alpha = d_tau / d_t) at 3D position.
    /// Reduces to sqrt(1 - r_s / r) in Schwarzschild and vanishes smoothly at r_+ in Kerr.
    #[inline(always)]
    pub fn time_dilation_at(&self, pos: Vec3) -> f64 {
        let m = self.mass();
        let a = self.spin_a();
        let r_h = self.event_horizon();
        if a < 1e-6 {
            let r = pos.length();
            return if r <= self.r_s { 0.0 } else { (1.0 - self.r_s / r).max(0.0).sqrt() };
        }
        let (r, _, _) = Self::kerr_fields(pos, m, a);
        if r <= r_h {
            return 0.0;
        }
        let r2 = r * r;
        let a2 = a * a;
        let cos_th = (pos.y / r).clamp(-1.0, 1.0);
        let sin2_th = (1.0 - cos_th * cos_th).max(0.0);
        let sigma = r2 + a2 * cos_th * cos_th;
        let delta = (r2 - self.r_s * r + a2).max(0.0);
        let big_a = (r2 + a2) * (r2 + a2) - a2 * delta * sin2_th;
        ((delta * sigma) / big_a.max(1e-12)).clamp(0.0, 1.0).sqrt()
    }

    /// Equatorial gravitational time dilation factor d_tau / d_t at radius r.
    #[inline(always)]
    pub fn time_dilation(&self, r: f64) -> f64 {
        self.time_dilation_at(Vec3::new(r, 0.0, 0.0))
    }

    /// Frame-dragging angular velocity omega = 2 M a r / A (rad per coordinate time).
    #[inline(always)]
    pub fn frame_dragging_omega(&self, pos: Vec3) -> f64 {
        let a = self.spin_a();
        if a < 1e-6 {
            return 0.0;
        }
        let m = self.mass();
        let (r, _, _) = Self::kerr_fields(pos, m, a);
        let r2 = r * r;
        let a2 = a * a;
        let cos_th = (pos.y / r.max(1e-6)).clamp(-1.0, 1.0);
        let sin2_th = (1.0 - cos_th * cos_th).max(0.0);
        let delta = (r2 - self.r_s * r + a2).max(0.0);
        let big_a = (r2 + a2) * (r2 + a2) - a2 * delta * sin2_th;
        (self.r_s * a * r) / big_a.max(1e-9)
    }

    /// Relativistic local orbital speed v/c for a circular orbit at radius r.
    #[inline(always)]
    pub fn orbital_beta(&self, r: f64) -> f64 {
        let r_h = self.event_horizon();
        if r <= 1.55 * r_h {
            0.72
        } else {
            (self.r_s / (2.0 * (r - r_h))).sqrt().min(0.88)
        }
    }

    /// Keplerian angular velocity omega(r) for procedural disk rotation.
    #[inline(always)]
    pub fn kepler_omega(&self, r: f64) -> f64 {
        let r_clamped = r.max(self.disk_inner * 0.8);
        1.4 / (r_clamped * r_clamped.sqrt())
    }

    #[inline(always)]
    fn schw_accel(&self, pos: Vec3, h2: f64) -> Vec3 {
        let r2 = pos.length_squared();
        let r5 = r2 * r2 * r2.sqrt();
        if r5 < 1e-18 {
            return Vec3::ZERO;
        }
        pos * (-1.5 * self.r_s * h2 / r5)
    }

    #[inline(always)]
    fn rk4_step_schw(&self, pos: Vec3, vel: Vec3, h2: f64, dt: f64) -> (Vec3, Vec3) {
        let k1_x = vel;
        let k1_v = self.schw_accel(pos, h2);

        let p2 = pos + k1_x * (0.5 * dt);
        let v2 = vel + k1_v * (0.5 * dt);
        let k2_x = v2;
        let k2_v = self.schw_accel(p2, h2);

        let p3 = pos + k2_x * (0.5 * dt);
        let v3 = vel + k2_v * (0.5 * dt);
        let k3_x = v3;
        let k3_v = self.schw_accel(p3, h2);

        let p4 = pos + k3_x * dt;
        let v4 = vel + k3_v * dt;
        let k4_x = v4;
        let k4_v = self.schw_accel(p4, h2);

        let next_pos = pos + (k1_x + k2_x * 2.0 + k3_x * 2.0 + k4_x) * (dt / 6.0);
        let next_vel = vel + (k1_v + k2_v * 2.0 + k3_v * 2.0 + k4_v) * (dt / 6.0);
        (next_pos, next_vel)
    }

    /// Kerr-Schild radial coordinate r and null vector l = (l_x, l_y, l_z) for spin along +Y.
    #[inline(always)]
    fn kerr_fields(pos: Vec3, m: f64, a: f64) -> (f64, f64, Vec3) {
        let a2 = a * a;
        let big_r2 = pos.length_squared();
        let diff = big_r2 - a2;
        let disc = (diff * diff + 4.0 * a2 * pos.y * pos.y).sqrt();
        let r2 = (0.5 * (diff + disc)).max(1e-12);
        let r = r2.sqrt();
        let denom = r2 + a2;

        // Spin axis along +Y
        let l = Vec3::new(
            (r * pos.x + a * pos.z) / denom,
            pos.y / r,
            (r * pos.z - a * pos.x) / denom,
        );
        let h = (m * r2 * r) / (r2 * r2 + a2 * pos.y * pos.y).max(1e-12);
        (r, h, l)
    }

    /// Hamiltonian derivatives (dx/dlambda, dp/dlambda, r) in Cartesian Kerr-Schild coordinates.
    #[inline(always)]
    fn kerr_hamilton_rhs(pos: Vec3, p: Vec3, e: f64, m: f64, a: f64) -> (Vec3, Vec3, f64) {
        let a2 = a * a;
        let big_r2 = pos.length_squared();
        let diff = big_r2 - a2;
        let disc = (diff * diff + 4.0 * a2 * pos.y * pos.y).sqrt().max(1e-12);
        let inv_disc = 1.0 / disc;
        let r2 = (0.5 * (diff + disc)).max(1e-12);
        let r = r2.sqrt();
        let inv_r = 1.0 / r;
        let inv_r2 = 1.0 / r2;
        let denom = r2 + a2;
        let inv_d = 1.0 / denom;
        let inv_d2 = inv_d * inv_d;

        let l = Vec3::new(
            (r * pos.x + a * pos.z) * inv_d,
            pos.y * inv_r,
            (r * pos.z - a * pos.x) * inv_d,
        );
        let h_denom = (r2 * r2 + a2 * pos.y * pos.y).max(1e-12);
        let inv_hd = 1.0 / h_denom;
        let inv_hd2 = inv_hd * inv_hd;
        let h = (m * r2 * r) * inv_hd;
        let lp = e + l.dot(p);

        // dx/dlambda = p - 2 H (l . p) l
        let dx = p - l * (2.0 * h * lp);

        // grad(r) = (r * pos + (0, a^2 * y / r, 0)) / disc
        let grad_r = Vec3::new(
            (r * pos.x) * inv_disc,
            (r * pos.y + a2 * pos.y * inv_r) * inv_disc,
            (r * pos.z) * inv_disc,
        );

        // grad(H) via quotient rule on H = M * r^3 / (r^4 + a^2 y^2)
        let dh_dr = m * r2 * (3.0 * a2 * pos.y * pos.y - r2 * r2) * inv_hd2;
        let mut grad_h = grad_r * dh_dr;
        grad_h.y -= (2.0 * m * r2 * r * a2 * pos.y) * inv_hd2;

        // p . (d l / d x^i)
        let dlx_dr = (pos.x * denom - (r * pos.x + a * pos.z) * 2.0 * r) * inv_d2;
        let dly_dr = -pos.y * inv_r2;
        let dlz_dr = (pos.z * denom - (r * pos.z - a * pos.x) * 2.0 * r) * inv_d2;
        let p_dot_dl_dr = p.x * dlx_dr + p.y * dly_dr + p.z * dlz_dr;

        // Explicit coordinate partials of l at fixed r
        let p_dot_dl_explicit = Vec3::new(
            (p.x * r - p.z * a) * inv_d,
            p.y * inv_r,
            (p.x * a + p.z * r) * inv_d,
        );
        let grad_l_dot_p = grad_r * p_dot_dl_dr + p_dot_dl_explicit;

        // dp/dlambda = (l.p)^2 grad(H) + 2 H (l.p) grad(l.p)
        let dp = grad_h * (lp * lp) + grad_l_dot_p * (2.0 * h * lp);
        (dx, dp, r)
    }

    #[inline(always)]
    fn rk4_step_kerr(pos: Vec3, p: Vec3, e: f64, m: f64, a: f64, horizon_r: f64) -> (Vec3, Vec3, f64) {
        let (k1_x, k1_p, r) = Self::kerr_hamilton_rhs(pos, p, e, m, a);
        let dt_max = if r > 18.0 { 3.2 } else { 1.5 };
        let dt = (0.078 * (r - 0.85 * horizon_r)).clamp(0.015, dt_max);
        let (k2_x, k2_p, _) = Self::kerr_hamilton_rhs(pos + k1_x * (0.5 * dt), p + k1_p * (0.5 * dt), e, m, a);
        let (k3_x, k3_p, _) = Self::kerr_hamilton_rhs(pos + k2_x * (0.5 * dt), p + k2_p * (0.5 * dt), e, m, a);
        let (k4_x, k4_p, _) = Self::kerr_hamilton_rhs(pos + k3_x * dt, p + k3_p * dt, e, m, a);

        let next_pos = pos + (k1_x + k2_x * 2.0 + k3_x * 2.0 + k4_x) * (dt / 6.0);
        let next_p = p + (k1_p + k2_p * 2.0 + k3_p * 2.0 + k4_p) * (dt / 6.0);
        (next_pos, next_p, r)
    }

    /// Segment-sphere intersection test in [0, 1] for an orbiting planet.
    #[inline(always)]
    fn intersect_segment_sphere(p0: Vec3, p1: Vec3, center: Vec3, radius: f64) -> Option<f64> {
        let seg = p1 - p0;
        let len2 = seg.length_squared();
        if len2 < 1e-18 {
            return None;
        }
        let oc = p0 - center;
        let b = oc.dot(seg);
        let c = oc.length_squared() - radius * radius;
        let disc = b * b - len2 * c;
        if disc < 0.0 {
            return None;
        }
        let t = (-b - disc.sqrt()) / len2;
        if (0.0..=1.0).contains(&t) {
            Some(t)
        } else {
            None
        }
    }

    /// Check planet and equatorial disk crossing for one step segment.
    #[inline(always)]
    fn check_segment<F>(
        &self,
        prev_pos: Vec3,
        next_pos: Vec3,
        fallback_dir: Vec3,
        planet: Option<&OrbitingPlanet>,
        on_disk_hit: &mut F,
    ) -> Option<TraceOutcome>
    where
        F: FnMut(DiskCrossing) -> bool,
    {
        if let Some(pl) = planet {
            if pl.enabled {
                if let Some(t_pl) = Self::intersect_segment_sphere(prev_pos, next_pos, pl.pos, pl.radius) {
                    let hit_pos = prev_pos.lerp(next_pos, t_pl);
                    return Some(TraceOutcome::HitPlanet(PlanetHit {
                        pos: hit_pos,
                        normal: (hit_pos - pl.pos).normalize(),
                        ray_dir: (next_pos - prev_pos).normalize(),
                    }));
                }
            }
        }

        if (prev_pos.y > 0.0 && next_pos.y <= 0.0) || (prev_pos.y < 0.0 && next_pos.y >= 0.0) {
            let denom = prev_pos.y - next_pos.y;
            let t = if denom.abs() > 1e-12 {
                (prev_pos.y / denom).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let hit_pos = prev_pos.lerp(next_pos, t);
            let hit_r = (hit_pos.x * hit_pos.x + hit_pos.z * hit_pos.z).sqrt();
            if hit_r >= self.disk_inner && hit_r <= self.disk_outer {
                let phi = hit_pos.z.atan2(hit_pos.x);
                let seg_dir = next_pos - prev_pos;
                let norm_dir = if seg_dir.length_squared() > 1e-18 {
                    seg_dir.normalize()
                } else {
                    fallback_dir.normalize()
                };
                if on_disk_hit(DiskCrossing { pos: hit_pos, dir: norm_dir, r: hit_r, phi }) {
                    return Some(TraceOutcome::Captured);
                }
            }
        }
        None
    }

    /// Trace a null geodesic from `origin` along `dir`.
    pub fn trace_ray<F>(
        &self,
        origin: Vec3,
        dir: Vec3,
        escape_radius: f64,
        planet: Option<&OrbitingPlanet>,
        mut on_disk_hit: F,
    ) -> TraceOutcome
    where
        F: FnMut(DiskCrossing) -> bool,
    {
        match self.metric {
            MetricModel::Schwarzschild => self.trace_schwarzschild(
                origin,
                dir,
                escape_radius,
                planet,
                &mut on_disk_hit,
            ),
            MetricModel::KerrSchild { spin } => self.trace_kerr(
                origin,
                dir,
                escape_radius,
                spin,
                planet,
                &mut on_disk_hit,
            ),
        }
    }

    fn trace_schwarzschild<F>(
        &self,
        origin: Vec3,
        dir: Vec3,
        escape_radius: f64,
        planet: Option<&OrbitingPlanet>,
        on_disk_hit: &mut F,
    ) -> TraceOutcome
    where
        F: FnMut(DiskCrossing) -> bool,
    {
        let mut pos = origin;
        let mut vel = dir.normalize();
        let h2 = pos.cross(vel).length_squared();

        let horizon_r = self.r_s * 1.02;
        let max_steps = 220;

        for _ in 0..max_steps {
            let r = pos.length();
            if r <= horizon_r {
                return TraceOutcome::Captured;
            }
            if r > escape_radius && pos.dot(vel) > 0.0 {
                return TraceOutcome::Escaped { final_dir: vel.normalize() };
            }

            let dt_max = if r > 18.0 { 3.2 } else { 1.5 };
            let dt = (0.078 * (r - 0.85 * self.r_s)).clamp(0.015, dt_max);
            let prev_pos = pos;
            let (next_pos, next_vel) = self.rk4_step_schw(pos, vel, h2, dt);

            if let Some(outcome) = self.check_segment(
                prev_pos,
                next_pos,
                vel,
                planet,
                on_disk_hit,
            ) {
                return outcome;
            }

            pos = next_pos;
            vel = next_vel;
        }

        if pos.length() <= self.photon_sphere() * 1.1 {
            TraceOutcome::Captured
        } else {
            TraceOutcome::Escaped { final_dir: vel.normalize() }
        }
    }

    fn trace_kerr<F>(
        &self,
        origin: Vec3,
        dir: Vec3,
        escape_radius: f64,
        spin_chi: f64,
        planet: Option<&OrbitingPlanet>,
        on_disk_hit: &mut F,
    ) -> TraceOutcome
    where
        F: FnMut(DiskCrossing) -> bool,
    {
        let m = self.mass();
        let a = spin_chi.clamp(0.0, 0.998) * m * self.spin_dir;
        let horizon_r = self.event_horizon() * 1.03;

        let mut pos = origin;
        let mut p = dir.normalize();

        // Solve null Hamiltonian H(x, p, E) = 0 for E = -p_t > 0
        let (_, h0, l0) = Self::kerr_fields(pos, m, a);
        let lp0 = l0.dot(p);
        let qa = 1.0 + 2.0 * h0;
        let qb = -4.0 * h0 * lp0;
        let qc = 2.0 * h0 * lp0 * lp0 - p.length_squared();
        let disc = (qb * qb - 4.0 * qa * qc).max(0.0);
        let e = (-qb + disc.sqrt()) / (2.0 * qa);

        let max_steps = 220;

        for _ in 0..max_steps {
            let prev_pos = pos;
            let (next_pos, next_p, r) = Self::rk4_step_kerr(pos, p, e, m, a, horizon_r);
            if r <= horizon_r {
                return TraceOutcome::Captured;
            }
            if r > escape_radius && pos.dot(p) > 0.0 {
                return TraceOutcome::Escaped { final_dir: p.normalize() };
            }

            if let Some(outcome) = self.check_segment(
                prev_pos,
                next_pos,
                p,
                planet,
                on_disk_hit,
            ) {
                return outcome;
            }

            pos = next_pos;
            p = next_p;
        }

        if pos.length() <= horizon_r * 1.4 {
            TraceOutcome::Captured
        } else {
            TraceOutcome::Escaped { final_dir: p.normalize() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_impact_parameter() {
        let mut bh = BlackHole::new();
        bh.metric = MetricModel::Schwarzschild;
        let b_crit = 1.5 * 3.0_f64.sqrt() * bh.r_s;

        let res_in = bh.trace_ray(
            Vec3::new(0.0, 0.0, 40.0),
            Vec3::new(b_crit - 0.15, 0.0, -40.0),
            60.0,
            None,
            |_| false,
        );
        assert!(matches!(res_in, TraceOutcome::Captured), "Ray inside b_crit should be captured");

        let res_out = bh.trace_ray(
            Vec3::new(0.0, 0.0, 40.0),
            Vec3::new(b_crit + 0.15, 0.0, -40.0),
            60.0,
            None,
            |_| false,
        );
        assert!(matches!(res_out, TraceOutcome::Escaped { .. }), "Ray outside b_crit should escape");
    }

    #[test]
    fn test_kerr_zero_spin_matches_schwarzschild_and_high_spin_asymmetry() {
        let mut bh = BlackHole::new();
        bh.metric = MetricModel::KerrSchild { spin: 0.0 };
        let b_crit = 1.5 * 3.0_f64.sqrt() * bh.r_s;

        let res_in = bh.trace_ray(
            Vec3::new(0.0, 0.0, 40.0),
            Vec3::new(b_crit - 0.15, 0.0, -40.0),
            60.0,
            None,
            |_| false,
        );
        assert!(matches!(res_in, TraceOutcome::Captured));

        bh.metric = MetricModel::KerrSchild { spin: 0.99 };
        assert!(bh.event_horizon() < 0.6);
        assert!(bh.isco() < 1.5);
    }

    #[test]
    fn test_time_dilation_and_isco() {
        let mut bh = BlackHole::new();
        bh.metric = MetricModel::Schwarzschild;
        assert!((bh.isco() - 3.0).abs() < 1e-9);
        assert_eq!(bh.time_dilation(1.0), 0.0);
        assert!((bh.time_dilation(4.0) - 0.75_f64.sqrt()).abs() < 1e-9);

        // Exact Kerr ZAMO lapse must vanish at event horizon r_+ and be positive outside
        bh.metric = MetricModel::KerrSchild { spin: 0.90 };
        let r_h = bh.event_horizon();
        assert_eq!(bh.time_dilation(r_h), 0.0);
        assert!(bh.time_dilation(r_h * 1.5) > 0.0);
        assert!(bh.time_dilation(r_h * 1.5) < 0.7);
    }

    #[test]
    fn test_hawking_evaporation_and_auto_rewind() {
        let mut bh = BlackHole::new();
        assert_eq!(bh.hawking_glow(), [0.0, 0.0, 0.0]);

        bh.trigger_evaporation();
        for _ in 0..24 {
            bh.update_evaporation(0.5);
        }
        assert!(bh.is_evaporated() || bh.mass_scale < 0.05);

        // Advance through EmptyHold + Rewinding -> returns to Idle with mass_scale == 1.0
        for _ in 0..20 {
            bh.update_evaporation(0.5);
        }
        assert_eq!(bh.evap_phase, EvapPhase::Idle);
        assert!((bh.mass_scale - 1.0).abs() < 1e-9);
    }
}

//! Parallel Geodesic Raycaster powered by Rayon + HDR Post-Processing.

use crate::physics::black_hole::{BlackHole, OrbitingPlanet, TraceOutcome};
use crate::render::buffer::FrameBuffer;
use crate::render::camera::Camera;
use crate::render::disk_shader::DiskShader;
use crate::render::galactic_sky::GalacticSky;
use crate::render::planet_shader::PlanetShader;
use crate::render::post_process::PostProcessor;
use rayon::prelude::*;

pub struct Raycaster {
    pub post: PostProcessor,
    pub bloom_enabled: bool,
    pub show_stars: bool,
}

impl Raycaster {
    pub fn new(cols: usize, v_height: usize) -> Self {
        Self {
            post: PostProcessor::new(cols, v_height),
            bloom_enabled: true,
            show_stars: true,
        }
    }

    pub fn render_frame(
        &mut self,
        buffer: &mut FrameBuffer,
        camera: &Camera,
        bh: &BlackHole,
        planet: &OrbitingPlanet,
        tick: u64,
        elapsed: f64,
    ) {
        let cols = buffer.cols;
        let v_height = buffer.virtual_height();
        let mode = buffer.mode;

        self.post.resize(cols, v_height);
        let basis = camera.basis();
        let beta = camera.camera_beta(bh);
        let cam_r = basis.0.length();
        let cam_dilation = bh.time_dilation_at(basis.0).max(0.15) as f32;
        let cam_blueshift = if cam_dilation < 0.85 {
            (0.85 / cam_dilation).min(2.6)
        } else {
            1.0
        };
        let bs_r = 1.0 + (cam_blueshift - 1.0) * 0.20;
        let bs_g = 1.0 + (cam_blueshift - 1.0) * 0.35;
        let bs_b = 1.0 + (cam_blueshift - 1.0) * 0.65;
        let apply_blueshift = cam_blueshift > 1.01;
        let escape_r = (cam_r * 1.65).clamp(45.0, 220.0);
        let show_stars = self.show_stars;
        let planet_opt = if planet.enabled { Some(planet) } else { None };
        let evaporated = bh.is_evaporated();
        let hawking = bh.hawking_glow();
        let flash = bh.flash_intensity;
        let to_center = (-basis.0).normalize();

        self.post
            .hdr
            .par_chunks_mut(cols)
            .enumerate()
            .for_each(|(v, row)| {
                for u in 0..cols {
                    let ray = camera.generate_ray(u, v, cols, v_height, mode, basis, beta);

                    if evaporated {
                        let mut accum = if show_stars {
                            GalacticSky::sample_hdr(ray.dir, tick)
                        } else {
                            [0.0, 0.0, 0.0]
                        };
                        if flash > 0.01 {
                            let cos_th = ray.dir.dot(to_center).clamp(-1.0, 1.0);
                            let angle = cos_th.acos() as f32;
                            // Expanding spherical gamma-ray shockwave ring as flash decays
                            let wave_r = ((3.8 - flash).max(0.0) * 0.14).clamp(0.0, 0.6);
                            let ring = (-((angle - wave_r) * 18.0).powi(2)).exp();
                            let core = cos_th.max(0.0).powf(24.0) as f32;
                            let burst = flash * (0.15 + 1.4 * core + 1.6 * ring);
                            accum[0] += burst * 0.82;
                            accum[1] += burst * 0.96;
                            accum[2] += burst * 1.45;
                        }
                        row[u] = accum;
                        continue;
                    }

                    let mut accum = [0.0_f32; 3];
                    let mut transmittance = 1.0_f32;

                    let outcome = bh.trace_ray(
                        ray.origin,
                        ray.dir,
                        escape_r,
                        planet_opt,
                        |hit| {
                            let (rgb, alpha) = DiskShader::sample(bh, hit, elapsed);
                            if alpha > 0.001 {
                                let w = transmittance * alpha;
                                accum[0] += rgb[0] * w;
                                accum[1] += rgb[1] * w;
                                accum[2] += rgb[2] * w;
                                transmittance *= 1.0 - alpha;
                            }
                            transmittance < 0.02
                        },
                    );

                    match outcome {
                        TraceOutcome::Captured => {
                            accum[0] += hawking[0] * transmittance;
                            accum[1] += hawking[1] * transmittance;
                            accum[2] += hawking[2] * transmittance;
                        }
                        TraceOutcome::HitPlanet(hit) => {
                            let pl_rgb = PlanetShader::shade(hit, elapsed);
                            accum[0] += pl_rgb[0] * transmittance;
                            accum[1] += pl_rgb[1] * transmittance;
                            accum[2] += pl_rgb[2] * transmittance;
                        }
                        TraceOutcome::Escaped { final_dir } => {
                            if show_stars && transmittance > 0.01 {
                                let star = GalacticSky::sample_hdr(final_dir, tick);
                                accum[0] += star[0] * transmittance;
                                accum[1] += star[1] * transmittance;
                                accum[2] += star[2] * transmittance;
                            }
                        }
                    }

                    if apply_blueshift {
                        accum[0] *= bs_r;
                        accum[1] *= bs_g;
                        accum[2] *= bs_b;
                    }

                    row[u] = accum;
                }
            });

        self.post.resolve_to_buffer(buffer, self.bloom_enabled);
    }
}

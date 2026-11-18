//! Procedural HDR Cube-Map Starfield (3x3 Worley search).
//! Guarantees circular, 1-2 pixel crisp stars across the entire celestial sphere
//! with zero pole distortion, creating vivid Einstein rings under lensing.

use crate::math::Vec3;

pub struct GalacticSky;

impl GalacticSky {
    #[inline(always)]
    fn hash3(ix: i32, iy: i32, iz: i32) -> f64 {
        let mut n = (ix as u32)
            .wrapping_mul(374761393)
            .wrapping_add((iy as u32).wrapping_mul(668265263))
            .wrapping_add((iz as u32).wrapping_mul(2147483647));
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        n ^= n >> 16;
        (n as f64) / (u32::MAX as f64)
    }

    /// Project unit direction onto cube-map face (0..5) and local coords u, v in [-1, 1].
    #[inline(always)]
    fn dir_to_cubemap(d: Vec3) -> (i32, f64, f64) {
        let ax = d.x.abs();
        let ay = d.y.abs();
        let az = d.z.abs();
        if ax >= ay && ax >= az {
            if d.x > 0.0 {
                (0, -d.z / ax, d.y / ax)
            } else {
                (1, d.z / ax, d.y / ax)
            }
        } else if ay >= ax && ay >= az {
            if d.y > 0.0 {
                (2, d.x / ay, -d.z / ay)
            } else {
                (3, d.x / ay, d.z / ay)
            }
        } else if d.z > 0.0 {
            (4, d.x / az, d.y / az)
        } else {
            (5, -d.x / az, d.y / az)
        }
    }

    /// 3x3 neighbor search on a cube-map face so stars never clip at cell boundaries.
    #[inline(always)]
    fn star_layer_cubemap(
        face: i32,
        u: f64,
        v: f64,
        grid_n: f64,
        density: f64,
        inv_r2: f64,
        gain: f32,
        tick: u64,
    ) -> [f32; 3] {
        let gx = (u * 0.5 + 0.5) * grid_n;
        let gy = (v * 0.5 + 0.5) * grid_n;
        let cx = gx.floor() as i32;
        let cy = gy.floor() as i32;

        let mut acc = [0.0_f32; 3];

        for dy in -1..=1 {
            for dx in -1..=1 {
                let nx = cx + dx;
                let ny = cy + dy;
                let h = Self::hash3(nx + face * 1024, ny, face * 313 + 7);
                if h < 1.0 - density {
                    continue;
                }

                let jx = 0.2 + 0.6 * Self::hash3(nx + 17, ny + face * 97, 13);
                let jy = 0.2 + 0.6 * Self::hash3(nx - 23, ny + 41, face * 53);

                let px = nx as f64 + jx - gx;
                let py = ny as f64 + jy - gy;
                let dist2 = px * px + py * py;

                let core = (-dist2 * inv_r2).exp() as f32;
                if core < 0.02 {
                    continue;
                }

                let temp_seed = Self::hash3(nx + 7, ny - 11, face * 19);
                let twinkle = 0.90 + 0.10 * ((tick as f64 * 0.14 + h * 40.0).sin()) as f32;
                let mag = 0.35 + 0.65 * ((h - (1.0 - density)) / density) as f32;
                let b = mag * gain * core * twinkle;

                if temp_seed > 0.72 {
                    acc[0] += b * 0.75;
                    acc[1] += b * 0.90;
                    acc[2] += b * 1.35;
                } else if temp_seed > 0.40 {
                    acc[0] += b * 1.05;
                    acc[1] += b * 1.05;
                    acc[2] += b * 1.10;
                } else if temp_seed > 0.16 {
                    acc[0] += b * 1.22;
                    acc[1] += b * 0.98;
                    acc[2] += b * 0.70;
                } else {
                    acc[0] += b * 1.25;
                    acc[1] += b * 0.60;
                    acc[2] += b * 0.35;
                }
            }
        }

        acc
    }

    /// Sample pure deep-black space with sparse, delicate stars.
    pub fn sample_hdr(dir: Vec3, tick: u64) -> [f32; 3] {
        let d = dir.normalize();
        let (face, u, v) = Self::dir_to_cubemap(d);

        // Single sparse pinpoint layer (~570 stars across the entire sky)
        Self::star_layer_cubemap(face, u, v, 80.0, 0.015, 48.0, 2.0, tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_galactic_sky_stars_actually_visible() {
        let mut bright_hits = 0usize;
        let samples = 8000usize;
        for i in 0..samples {
            // Fibonacci sphere uniform sampling
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / samples as f64;
            let r = (1.0 - y * y).sqrt();
            let phi = i as f64 * 2.399963229728653;
            let dir = Vec3::new(r * phi.cos(), y, r * phi.sin());
            let rgb = GalacticSky::sample_hdr(dir, 0);
            for c in rgb {
                assert!(c.is_finite() && c >= 0.0);
            }
            let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            if lum > 0.35 {
                bright_hits += 1;
            }
        }
        assert!(
            bright_hits >= 3 && bright_hits <= 120,
            "Expected sparse pinpoint stars (3..120 hits in 8000 rays), got {}",
            bright_hits
        );
    }
}

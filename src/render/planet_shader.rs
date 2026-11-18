//! S2 Star Shader: bright blue-white B0V star orbiting Sagittarius A*.

use crate::physics::black_hole::PlanetHit;

pub struct PlanetShader;

impl PlanetShader {
    pub fn shade(hit: PlanetHit, elapsed: f64) -> [f32; 3] {
        let n = hit.normal;
        let v = -hit.ray_dir.normalize();
        let mu = n.dot(v).clamp(0.0, 1.0) as f32;

        // Limb darkening + subtle stellar granulation
        let gran = ((hit.pos.x * 16.0 + elapsed * 1.8).sin()
            * (hit.pos.y * 16.0 - elapsed * 1.4).cos()) as f32
            * 0.08;
        let core = (0.45 + 0.55 * mu.powf(0.6) + gran).max(0.2);

        // B0V blue-white star HDR emission (triggers bloom + crisp Einstein arcs)
        [core * 1.65, core * 2.05, core * 2.95]
    }
}

//! Real-world CPU rendering benchmark across terminal resolutions and metrics.
//! Run with: `cargo run --release --example bench`

use black_hole_ascii_simulation::physics::black_hole::{BlackHole, MetricModel, OrbitingPlanet};
use black_hole_ascii_simulation::render::buffer::{FrameBuffer, RenderMode};
use black_hole_ascii_simulation::render::camera::Camera;
use black_hole_ascii_simulation::render::raycaster::Raycaster;
use std::time::Instant;

fn bench_config(name: &str, cols: usize, lines: usize, metric: MetricModel) {
    let mut buffer = FrameBuffer::new(cols, lines, RenderMode::HalfBlock);
    let mut raycaster = Raycaster::new(cols, buffer.virtual_height());
    let mut camera = Camera::new();
    camera.zen_mode = false;
    camera.intro_active = false;

    let mut bh = BlackHole::new();
    bh.metric = metric;
    bh.sync_disk_inner();

    let planet = OrbitingPlanet::s2_star();

    // Warmup
    for i in 0..3 {
        raycaster.render_frame(&mut buffer, &camera, &bh, &planet, i, i as f64 * 0.03);
        let _ = buffer.flush_to_bytes();
    }

    let frames = 20;
    let mut times_ms = Vec::with_capacity(frames);
    for i in 0..frames {
        let t0 = Instant::now();
        raycaster.render_frame(
            &mut buffer,
            &camera,
            &bh,
            &planet,
            i as u64,
            i as f64 * 0.03,
        );
        let _ = buffer.flush_to_bytes();
        times_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let avg = times_ms.iter().sum::<f64>() / frames as f64;
    let min = times_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let max = times_ms.iter().copied().fold(0.0_f64, f64::max);
    let fps = 1000.0 / avg;
    let pixels = cols * buffer.virtual_height();

    println!(
        "{:<24} | {:>3}x{:<3} ({:>5} px) | avg {:>6.2} ms | min {:>5.2} / max {:>5.2} ms | {:>5.1} FPS",
        name, cols, lines, pixels, avg, min, max, fps
    );
}

fn main() {
    println!("=== Black Hole Geodesic Raycaster Benchmark (HalfBlock + Bloom + ACES + GalacticSky) ===");
    let sizes = [(80, 24), (120, 40), (180, 50), (220, 65)];

    for &(cols, lines) in &sizes {
        bench_config("Schwarzschild (a=0)", cols, lines, MetricModel::Schwarzschild);
    }
    println!("{}", "-".repeat(95));
    for &(cols, lines) in &sizes {
        bench_config(
            "Kerr-Schild (a=0.90M)",
            cols,
            lines,
            MetricModel::KerrSchild { spin: 0.90 },
        );
    }
}

//! Main 60 FPS interactive simulation loop for the Relativistic Black Hole Simulator.

use crate::physics::black_hole::{BlackHole, DopplerMode, MetricModel, OrbitingPlanet};
use crate::render::buffer::{FrameBuffer, RenderMode};
use crate::render::camera::{Camera, CameraPreset};
use crate::render::hud::{Hud, Lang};
use crate::render::raycaster::Raycaster;
use crate::terminal::{TerminalEvent, TerminalManager};
use crossterm::event::{KeyCode, KeyModifiers};
use std::io::{stdout, Write};
use std::time::{Duration, Instant};

pub struct App {
    pub term: TerminalManager,
    pub black_hole: BlackHole,
    pub planet: OrbitingPlanet,
    pub raycaster: Raycaster,
    pub camera: Camera,
    pub buffer: FrameBuffer,
    pub time_scale: f64,
    pub paused: bool,
    pub sim_time: f64,
    pub local_time: f64,
    pub far_time: f64,
    pub tick: u64,
    pub show_hud: bool,
    pub show_help: bool,
    pub lang: Lang,
}

impl App {
    pub fn new(initial_mode: RenderMode, lang: Lang) -> Self {
        let (cols, lines) = TerminalManager::size();
        let buffer = FrameBuffer::new(cols, lines, initial_mode);
        let v_height = buffer.virtual_height();

        Self {
            term: TerminalManager::new(),
            black_hole: BlackHole::new(),
            planet: OrbitingPlanet::s2_star(),
            raycaster: Raycaster::new(cols, v_height),
            camera: Camera::new(),
            buffer,
            time_scale: 1.0,
            paused: false,
            sim_time: 0.0,
            local_time: 0.0,
            far_time: 0.0,
            tick: 0,
            show_hud: true,
            show_help: false,
            lang,
        }
    }

    pub fn run(&mut self) {
        self.term.init();
        let mut out = stdout();
        let mut last_frame = Instant::now();
        let mut fps = 60.0;
        let target_dt = Duration::from_secs_f64(1.0 / 60.0);

        'main: loop {
            let frame_start = Instant::now();
            let dt = (frame_start - last_frame).as_secs_f64().clamp(0.001, 0.1);
            last_frame = frame_start;
            fps = fps * 0.9 + (1.0 / dt) * 0.1;

            let (cols, lines) = TerminalManager::size();
            self.buffer.resize(cols, lines);
            let min_r = (self.black_hole.event_horizon() * 1.05).max(0.45);

            while let Some(ev) = self.term.poll_event() {
                match ev {
                    TerminalEvent::Key(code, mods) => {
                        if code == KeyCode::Char('q')
                            || code == KeyCode::Char('Q')
                            || code == KeyCode::Esc
                            || (code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL))
                        {
                            break 'main;
                        }
                        self.handle_key(code, min_r);
                    }
                    TerminalEvent::MouseDrag { dx, dy } => {
                        self.camera.orbit(-dx as f64 * 0.008, dy as f64 * 0.012);
                    }
                    TerminalEvent::MouseScrollUp => self.camera.zoom(0.88, min_r),
                    TerminalEvent::MouseScrollDown => self.camera.zoom(1.13, min_r),
                    TerminalEvent::MouseClick { .. } => {}
                }
            }

            if !self.paused {
                let sim_dt = dt * self.time_scale;
                let cam_dilation = self.black_hole.time_dilation_at(self.camera.position()).max(0.05);
                self.sim_time += sim_dt;
                self.local_time += sim_dt;
                self.far_time += sim_dt / cam_dilation;
                self.planet.update(sim_dt);
                self.black_hole.update_evaporation(sim_dt);
            }
            self.camera.update(dt, &self.black_hole);
            self.tick = self.tick.wrapping_add(1);

            self.raycaster.render_frame(
                &mut self.buffer,
                &self.camera,
                &self.black_hole,
                &self.planet,
                self.tick,
                self.sim_time,
            );

            if self.show_hud {
                Hud::render(
                    &mut self.buffer,
                    &self.camera,
                    &self.black_hole,
                    fps,
                    self.local_time,
                    self.far_time,
                    self.paused,
                    self.planet.enabled,
                    self.show_help,
                    self.lang,
                );
            }

            let bytes = self.buffer.flush_to_bytes();
            let _ = out.write_all(bytes);
            let _ = out.flush();

            let elapsed = frame_start.elapsed();
            if elapsed < target_dt {
                std::thread::sleep(target_dt - elapsed);
            }
        }

        self.term.restore();
    }

    fn handle_key(&mut self, code: KeyCode, min_r: f64) {
        match code {
            KeyCode::Char('1') => self.camera.apply_preset(CameraPreset::EdgeOn),
            KeyCode::Char('2') => self.camera.apply_preset(CameraPreset::PhotonRing),
            KeyCode::Char('3') => self.camera.apply_preset(CameraPreset::PolarView),
            KeyCode::Char('4') => self.camera.apply_preset(CameraPreset::CloseHorizon),
            KeyCode::Char('z') | KeyCode::Char('Z') => {
                self.camera.intro_active = false;
                self.camera.transition = None;
                self.camera.zen_mode = !self.camera.zen_mode;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.black_hole.trigger_evaporation();
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                self.black_hole.metric = match self.black_hole.metric {
                    MetricModel::Schwarzschild => MetricModel::KerrSchild { spin: 0.90 },
                    MetricModel::KerrSchild { .. } => MetricModel::Schwarzschild,
                };
                self.black_hole.sync_disk_inner();
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.planet.enabled = !self.planet.enabled;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.black_hole.doppler_mode = match self.black_hole.doppler_mode {
                    DopplerMode::Cinematic => DopplerMode::Physical,
                    DopplerMode::Physical => DopplerMode::Cinematic,
                };
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                let next = match self.buffer.mode {
                    RenderMode::HalfBlock => RenderMode::Ascii,
                    RenderMode::Ascii => RenderMode::HalfBlock,
                };
                self.buffer.set_mode(next);
            }
            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('?') | KeyCode::Char('/') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char('u') | KeyCode::Char('U') => self.show_hud = !self.show_hud,
            KeyCode::Char(' ') => self.paused = !self.paused,
            KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.camera.zoom(0.88, min_r);
            }
            KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Char('-') | KeyCode::Char('_') => {
                self.camera.zoom(1.13, min_r);
            }
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => self.camera.orbit(-0.08, 0.0),
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => self.camera.orbit(0.08, 0.0),
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => self.camera.orbit(0.0, 0.06),
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => self.camera.orbit(0.0, -0.06),
            _ => {}
        }
    }
}

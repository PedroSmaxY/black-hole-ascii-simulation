//! Relativistic Telemetry HUD & Help Modal with automatic PT/EN localization.

use crate::physics::black_hole::{BlackHole, DopplerMode, EvapPhase, MetricModel};
use crate::render::buffer::{FrameBuffer, Pixel, RenderMode};
use crate::render::camera::Camera;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Pt,
    En,
}

impl Lang {
    pub fn from_locale_str(s: &str) -> Self {
        let lower = s.trim().to_ascii_lowercase();
        if lower.starts_with("pt") {
            Lang::Pt
        } else {
            Lang::En
        }
    }

    pub fn detect(args: &[String]) -> Self {
        for w in args.windows(2) {
            if w[0] == "--lang" {
                return Self::from_locale_str(&w[1]);
            }
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if !val.is_empty() {
                    return Self::from_locale_str(&val);
                }
            }
        }
        if let Some(os_loc) = Self::os_locale() {
            return Self::from_locale_str(&os_loc);
        }
        Lang::En
    }

    #[cfg(windows)]
    fn os_locale() -> Option<String> {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(lp_locale_name: *mut u16, cch_locale_name: i32) -> i32;
        }
        let mut buf = [0u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if len > 1 {
            Some(String::from_utf16_lossy(&buf[..(len as usize - 1)]))
        } else {
            None
        }
    }

    #[cfg(target_os = "macos")]
    fn os_locale() -> Option<String> {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            .ok()?;
        if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            None
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn os_locale() -> Option<String> {
        None
    }
}

pub struct Hud;

impl Hud {
    fn draw_text(buffer: &mut FrameBuffer, x: usize, y: usize, text: &str, color: [u8; 3]) {
        if y >= buffer.lines {
            return;
        }
        let v_y = match buffer.mode {
            RenderMode::Ascii => y,
            RenderMode::HalfBlock => y * 2,
        };
        for (i, c) in text.chars().enumerate() {
            let px = x + i;
            if px < buffer.cols {
                buffer.set_pixel(px, v_y, Pixel::text(color[0], color[1], color[2], c));
                if buffer.mode == RenderMode::HalfBlock && v_y + 1 < buffer.virtual_height() {
                    let bot_idx = (v_y + 1) * buffer.cols + px;
                    buffer.pixels[bot_idx].r /= 4;
                    buffer.pixels[bot_idx].g /= 4;
                    buffer.pixels[bot_idx].b /= 4;
                }
            }
        }
    }

    fn format_clock(secs: f64) -> String {
        let mins = (secs / 60.0).floor() as u64;
        let rem = secs % 60.0;
        format!("{:02}:{:04.1}", mins, rem)
    }

    pub fn render(
        buffer: &mut FrameBuffer,
        camera: &Camera,
        bh: &BlackHole,
        fps: f64,
        local_time: f64,
        far_time: f64,
        paused: bool,
        planet_enabled: bool,
        show_help: bool,
        lang: Lang,
    ) {
        let cols = buffer.cols;
        let lines = buffer.lines;
        if cols < 24 || lines < 6 {
            return;
        }

        let cam_pos = camera.position();
        let cam_r = cam_pos.length();
        let dilation = bh.time_dilation_at(cam_pos);
        let far_rate = if dilation > 0.02 { 1.0 / dilation } else { 99.9 };

        let doppler_str = match (bh.doppler_mode, lang) {
            (DopplerMode::Cinematic, _) => "Cine",
            (DopplerMode::Physical, Lang::Pt) => "Fís",
            (DopplerMode::Physical, Lang::En) => "Phys",
        };
        let metric_str = match (bh.metric, lang) {
            (MetricModel::Schwarzschild, Lang::Pt) => "Lite · Sem Giro".to_string(),
            (MetricModel::Schwarzschild, Lang::En) => "Lite · No Spin".to_string(),
            (MetricModel::KerrSchild { spin }, Lang::Pt) => format!("Kerr · Giro {:.2}M", spin),
            (MetricModel::KerrSchild { spin }, Lang::En) => format!("Kerr · Spin {:.2}M", spin),
        };
        let nav_str = if camera.intro_active {
            match lang { Lang::Pt => "Mergulho Inicial", Lang::En => "Intro Dive" }
        } else if camera.transition.is_some() {
            match lang { Lang::Pt => "Voando...", Lang::En => "Flying..." }
        } else if camera.zen_mode {
            "Zen Dive"
        } else {
            camera.active_preset_name
        };
        let pause_tag = if paused {
            match lang { Lang::Pt => " · PAUSADO", Lang::En => " · PAUSED" }
        } else {
            ""
        };

        let evap_tag = match (bh.evap_phase, lang) {
            (EvapPhase::Idle, _) => "".to_string(),
            (EvapPhase::Evaporating(_), Lang::Pt) => format!(" · [EVAP HAWKING M={:.2}]", bh.mass_scale),
            (EvapPhase::Evaporating(_), Lang::En) => format!(" · [HAWKING EVAP M={:.2}]", bh.mass_scale),
            (EvapPhase::EmptyHold(_), Lang::Pt) => " · [EVAPORADO · PICO T_H]".to_string(),
            (EvapPhase::EmptyHold(_), Lang::En) => " · [EVAPORATED · T_H PEAK]".to_string(),
            (EvapPhase::Rewinding(_), Lang::Pt) => format!(" · [<< REBOBINANDO M={:.2}]", bh.mass_scale),
            (EvapPhase::Rewinding(_), Lang::En) => format!(" · [<< REWINDING M={:.2}]", bh.mass_scale),
        };

        let danger_tag = if !evap_tag.is_empty() {
            evap_tag
        } else if cam_r < bh.r_s * 1.15 && bh.spin_a() > 0.01 {
            match lang { Lang::Pt => " · [ERGOSFERA]".to_string(), Lang::En => " · [ERGOSPHERE]".to_string() }
        } else if cam_r < 2.2 {
            match lang { Lang::Pt => " · [FORÇA DE MARÉ]".to_string(), Lang::En => " · [TIDAL SHEAR]".to_string() }
        } else {
            "".to_string()
        };

        let bh_label = match lang { Lang::Pt => "Buraco Negro", Lang::En => "Black Hole" };
        let title_full = format!(
            "{} · [{}] · {} · r = {:.2} r_s{}",
            bh_label, metric_str, nav_str, cam_r, danger_tag
        );
        let local_str = Self::format_clock(local_time);
        let far_str = Self::format_clock(far_time);
        let (you_lbl, earth_lbl, star_lbl) = match lang {
            Lang::Pt => ("Você", "Terra", "Estrela"),
            Lang::En => ("You", "Earth", "Star"),
        };
        let status_full = format!(
            "{} {} · {} {} ({:.2}x) · Doppler: {} · {}: {}{} · {:.0} FPS",
            you_lbl,
            local_str,
            earth_lbl,
            far_str,
            far_rate,
            doppler_str,
            star_lbl,
            if planet_enabled { "ON" } else { "OFF" },
            pause_tag,
            fps
        );
        let status_compact = format!("{} {} · {} {}{} · {:.0} FPS", you_lbl, local_str, earth_lbl, far_str, pause_tag, fps);

        let title_len = title_full.chars().count();
        let status_len = status_full.chars().count();

        if cols > title_len + status_len + 4 {
            Self::draw_text(buffer, 1, 0, &title_full, [255, 215, 140]);
            Self::draw_text(buffer, cols - status_len - 1, 0, &status_full, [195, 215, 235]);
        } else {
            let s_len = status_compact.chars().count();
            let avail_title = cols.saturating_sub(s_len + 3);
            let title_short: String = format!("{} · {}{}", bh_label, metric_str, danger_tag)
                .chars()
                .take(avail_title)
                .collect();
            Self::draw_text(buffer, 1, 0, &title_short, [255, 215, 140]);
            if cols > s_len + 2 {
                Self::draw_text(buffer, cols - s_len - 1, 0, &status_compact, [195, 215, 235]);
            }
        }

        // Welcome Callout (Line 1) during the first 6 seconds
        if local_time <= 6.0 && !paused && !show_help && lines > 4 {
            let welcome = match lang {
                Lang::Pt => "✦ Simulador de Buraco Negro · [WASD/Setas] Orbitar  [I/O] Zoom  [E] Hawking  [?/H] Ajuda ✦",
                Lang::En => "✦ Black Hole Relativistic Simulator · [WASD/Arrows] Orbit  [I/O] Dive  [E] Hawking  [?/H] Help ✦",
            };
            let w_len = welcome.chars().count();
            if cols >= w_len + 2 {
                let wx = (cols - w_len) / 2;
                Self::draw_text(buffer, wx, 1, welcome, [255, 215, 110]);
            }
        }

        // Responsive Footer
        if lines > 1 {
            let (footer_full, footer_comp) = match lang {
                Lang::Pt => (
                    "[WASD/Setas] Orbitar  [I/O] Zoom  [1-4] Vistas  [Z] Zen  [E] Hawking  [K] +FPS  [M] ASCII/+FPS  [V] Luz  [?/] Ajuda",
                    "[WASD] Orbitar  [I/O] Zoom  [1-4] Vistas  [Z] Zen  [E] Hawking  [M] ASCII  [?/] Ajuda",
                ),
                Lang::En => (
                    "[WASD/Arrows] Orbit  [I/O] Zoom  [1-4] Views  [Z] Auto  [E] Hawking  [K] +FPS  [M] ASCII/+FPS  [V] Light  [?/] Help",
                    "[WASD] Orbit  [I/O] Zoom  [1-4] Views  [Z] Auto  [E] Hawking  [M] ASCII  [?/] Help",
                ),
            };
            if cols >= footer_full.chars().count() + 2 {
                Self::draw_text(buffer, 1, lines - 1, footer_full, [160, 195, 225]);
            } else if cols >= footer_comp.chars().count() + 2 {
                Self::draw_text(buffer, 1, lines - 1, footer_comp, [160, 195, 225]);
            }
        }

        if show_help {
            Self::render_help_modal(buffer, lang);
        }
    }

    pub fn render_help_modal(buffer: &mut FrameBuffer, lang: Lang) {
        let cols = buffer.cols;
        let lines = buffer.lines;
        let modal_w = 68.min(cols.saturating_sub(4));
        let lines_text: [&str; 12] = match lang {
            Lang::Pt => [
                "┌────────────────── Black Hole Simulator Controls ─────────────────┐",
                "│ Girar Câmera     [W/A/S/D] ou [Setas] ou Arrastar com Mouse      │",
                "│ Aproximar / Sair [I] / [O] ou [+]/[-] ou Scroll (vai ao horizon) │",
                "│ Vistas Prontas   [1] Lado  [2] Anel  [3] Cima  [4] Bem Perto     │",
                "│ Piloto Autom.    [Z] Gira e mergulha sozinho (Modo Zen)          │",
                "│ Radiação Hawking [E] Acelera eras: evapora até sumir e rebobina  │",
                "│ Mais Leve (+FPS) [K] Desliga giro (Lite) para dobrar o FPS       │",
                "│ Estilo ASCII     [M] Alterna HD <-> ASCII Clássico (+Leve/+FPS)  │",
                "│ Estilo da Luz    [V] Físico Real (Azul/Escuro) <-> Cinema        │",
                "│ Extras           [P] Estrela distante  [U] Esconder textos       │",
                "│ Pausar / Sair    [Espaço] Pausar   |   [Q] ou [Esc] Sair         │",
                "└────────────── Aperte [?], [/], [H] ou [Esc] para fechar ─────────┘",
            ],
            Lang::En => [
                "┌────────────────── Black Hole Simulator Controls ─────────────────┐",
                "│ Orbit Camera     [W/A/S/D] or [Arrows] or Left-Click Drag        │",
                "│ Zoom / Dive      [I] / [O] or [+]/[-] or Scroll (to horizon!)    │",
                "│ Camera Views     [1] Edge  [2] Ring  [3] Top  [4] Close Horizon  │",
                "│ Autopilot        [Z] Smooth orbit & dive (Zen Mode)              │",
                "│ Hawking Evap     [E] Fast-forward eras: evaporate & rewind time  │",
                "│ Lighter (+FPS)   [K] Disable spin (Lite) to double frame rate    │",
                "│ ASCII Style      [M] Toggle HD <-> Classic ASCII (+Lighter/+FPS) │",
                "│ Light Style      [V] Physical (Blue/Dark) <-> Cinematic (Gold)   │",
                "│ Extras           [P] Distant companion star  [U] Hide HUD        │",
                "│ Pause / Quit     [Space] Pause     |   [Q] or [Esc] Quit         │",
                "└────────────── Press [?], [/], [H], or [Esc] to Close ────────────┘",
            ],
        };
        let modal_h = lines_text.len().min(lines.saturating_sub(2));
        if modal_w < 30 || modal_h < 6 {
            return;
        }

        let start_x = (cols - modal_w) / 2;
        let start_y = (lines - modal_h) / 2;

        for dy in 0..modal_h {
            let cur_y = start_y + dy;
            let v_y0 = match buffer.mode {
                RenderMode::Ascii => cur_y,
                RenderMode::HalfBlock => cur_y * 2,
            };
            let v_y1 = match buffer.mode {
                RenderMode::Ascii => cur_y,
                RenderMode::HalfBlock => cur_y * 2 + 1,
            };
            for x in 0..modal_w {
                let px = start_x + x;
                if px < buffer.cols {
                    buffer.set_pixel(px, v_y0, Pixel::rgb(12, 16, 26));
                    if buffer.mode == RenderMode::HalfBlock {
                        buffer.set_pixel(px, v_y1, Pixel::rgb(12, 16, 26));
                    }
                }
            }
        }

        for (idx, line) in lines_text.iter().enumerate() {
            if idx >= modal_h {
                break;
            }
            let y = start_y + idx;
            let trimmed: String = line.chars().take(modal_w).collect();
            let color = if idx == 0 || idx == lines_text.len() - 1 {
                [255, 210, 125]
            } else {
                [220, 230, 245]
            };
            Self::draw_text(buffer, start_x, y, &trimmed, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_detection() {
        assert_eq!(Lang::from_locale_str("pt_BR.UTF-8"), Lang::Pt);
        assert_eq!(Lang::from_locale_str("pt_PT"), Lang::Pt);
        assert_eq!(Lang::from_locale_str("en_US.UTF-8"), Lang::En);
        assert_eq!(
            Lang::detect(&["bin".into(), "--lang".into(), "pt".into()]),
            Lang::Pt
        );
        assert_eq!(
            Lang::detect(&["bin".into(), "--lang".into(), "en".into()]),
            Lang::En
        );
    }
}

//! HDR Linear Buffer, Separable 2D Bloom, and Parallel ACES Filmic Tone Mapping.

use crate::render::buffer::{FrameBuffer, Pixel};
use rayon::prelude::*;

const ASCII_RAMP: &[u8] = b" .:-=+*x%#@";

pub struct PostProcessor {
    pub width: usize,
    pub height: usize,
    pub hdr: Vec<[f32; 3]>,
    bright: Vec<[f32; 3]>,
    scratch: Vec<[f32; 3]>,
}

impl PostProcessor {
    pub fn new(width: usize, height: usize) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            hdr: vec![[0.0; 3]; len],
            bright: vec![[0.0; 3]; len],
            scratch: vec![[0.0; 3]; len],
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            let len = width * height;
            self.hdr.resize(len, [0.0; 3]);
            self.bright.resize(len, [0.0; 3]);
            self.scratch.resize(len, [0.0; 3]);
        }
    }

    #[inline(always)]
    fn aces_filmic(x: f32) -> f32 {
        let a = 2.51;
        let b = 0.03;
        let c = 2.43;
        let d = 0.59;
        let e = 0.14;
        ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
    }

    /// Run separable bloom + ACES filmic tone mapping and write into `FrameBuffer`.
    pub fn resolve_to_buffer(&mut self, buffer: &mut FrameBuffer, bloom_enabled: bool) {
        let w = self.width;
        let h = self.height;
        if w == 0 || h == 0 || self.hdr.len() != buffer.pixels.len() {
            return;
        }

        let mut has_bright = false;
        if bloom_enabled {
            // 1. Extract HDR highlights above threshold 1.35 (keeps black shadow pure black)
            let thresh = 1.35_f32;
            for (dst, src) in self.bright.iter_mut().zip(self.hdr.iter()) {
                let peak = src[0].max(src[1]).max(src[2]);
                if peak > thresh {
                    has_bright = true;
                    let scale = (peak - thresh) / peak.max(1e-5);
                    *dst = [src[0] * scale, src[1] * scale, src[2] * scale];
                } else {
                    *dst = [0.0, 0.0, 0.0];
                }
            }

            if has_bright {
                // 2. Parallel separable horizontal tent blur (radius 3)
                let radius: isize = 3;
                let bright_ref = &self.bright;
                self.scratch
                    .par_chunks_mut(w)
                    .enumerate()
                    .for_each(|(y, out_row)| {
                        let row = y * w;
                        for x in 0..w {
                            let mut acc = [0.0_f32; 3];
                            let mut wsum = 0.0_f32;
                            for dx in -radius..=radius {
                                let sx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                                let weight = (radius + 1 - dx.abs()) as f32;
                                let p = bright_ref[row + sx];
                                acc[0] += p[0] * weight;
                                acc[1] += p[1] * weight;
                                acc[2] += p[2] * weight;
                                wsum += weight;
                            }
                            let inv = 1.0 / wsum;
                            out_row[x] = [acc[0] * inv, acc[1] * inv, acc[2] * inv];
                        }
                    });

                // 3. Parallel separable vertical tent blur (radius 3)
                let scratch_ref = &self.scratch;
                self.bright
                    .par_chunks_mut(w)
                    .enumerate()
                    .for_each(|(y, out_row)| {
                        for x in 0..w {
                            let mut acc = [0.0_f32; 3];
                            let mut wsum = 0.0_f32;
                            for dy in -radius..=radius {
                                let sy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                                let weight = (radius + 1 - dy.abs()) as f32;
                                let p = scratch_ref[sy * w + x];
                                acc[0] += p[0] * weight;
                                acc[1] += p[1] * weight;
                                acc[2] += p[2] * weight;
                                wsum += weight;
                            }
                            let inv = 1.0 / wsum;
                            out_row[x] = [acc[0] * inv, acc[1] * inv, acc[2] * inv];
                        }
                    });
            }
        }

        // 4. Parallel Composite + ACES Tone Map + sRGB gamma + ASCII ramp (with zero fast-path)
        let bloom_strength = if bloom_enabled && has_bright { 0.30_f32 } else { 0.0_f32 };
        let inv_gamma = 1.0 / 2.2_f32;
        let hdr = &self.hdr;
        let bright = &self.bright;

        buffer
            .pixels
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, pix)| {
                let base = hdr[i];
                let glow = bright[i];
                let lr = (base[0] + glow[0] * bloom_strength).max(0.0);
                let lg = (base[1] + glow[1] * bloom_strength).max(0.0);
                let lb = (base[2] + glow[2] * bloom_strength).max(0.0);

                if lr == 0.0 && lg == 0.0 && lb == 0.0 {
                    *pix = Pixel::BLACK;
                    return;
                }

                let r = Self::aces_filmic(lr).powf(inv_gamma);
                let g = Self::aces_filmic(lg).powf(inv_gamma);
                let b = Self::aces_filmic(lb).powf(inv_gamma);

                let r_u8 = (r * 255.0).clamp(0.0, 255.0) as u8;
                let g_u8 = (g * 255.0).clamp(0.0, 255.0) as u8;
                let b_u8 = (b * 255.0).clamp(0.0, 255.0) as u8;

                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let ramp_idx = ((lum * (ASCII_RAMP.len() - 1) as f32).round() as usize)
                    .min(ASCII_RAMP.len() - 1);
                let ch = ASCII_RAMP[ramp_idx] as char;

                *pix = Pixel::new(r_u8, g_u8, b_u8, ch);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::buffer::RenderMode;

    #[test]
    fn test_bloom_and_aces_no_nan() {
        let mut pp = PostProcessor::new(8, 8);
        let mut fb = FrameBuffer::new(8, 4, RenderMode::HalfBlock);
        pp.hdr[27] = [25.0, 12.0, 3.0]; // intense photon ring highlight
        pp.resolve_to_buffer(&mut fb, true);

        assert!(fb.pixels[27].r > 200);
        assert!(fb.pixels[27].ch != ' ');
        // Neighbor should receive bloom glow
        assert!(fb.pixels[28].r > 0);
    }
}

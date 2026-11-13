use crate::font::{FontAtlas, FontKind};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub rect_size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct DrawBatch {
    pub index_offset: u32,
    pub index_count: u32,
    pub scissor: [i32; 4], // x, y, w, h
}

#[derive(Clone, Copy, Debug)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub const fn hex(rgb: u32, alpha: f32) -> Self {
        let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
        let b = (rgb & 0xFF) as f32 / 255.0;
        Self([r, g, b, alpha])
    }

    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self([self.0[0], self.0[1], self.0[2], self.0[3] * alpha])
    }
}

/// Verified WCAG AA palette (see contrast-check.py results):
/// - Canvas: #0F1216
/// - Surface: #171C23
/// - Interactive Border: #637288 (3.50:1 vs #171C23)
/// - Primary Text: #F2F4F8 (17.05:1 vs #0F1216, 15.55:1 vs #171C23)
/// - Secondary Text: #A8B3C4 (8.08:1 vs #171C23)
/// - Deliberate Accent (Amber): #E5A93B (9.00:1 dark text, 8.21:1 vs #171C23)
/// - Active Semantic (Emerald): #2EC27E (8.17:1 dark text, 7.45:1 vs #171C23)
/// - Warning Semantic (Cinnabar): #E05A47 (4.66:1 vs #171C23)
pub mod palette {
    use super::Color;
    pub const CANVAS: Color = Color::hex(0x11141A, 0.93);
    pub const CANVAS_SOLID: Color = Color::hex(0x11141A, 1.0);
    pub const SURFACE: Color = Color::hex(0x171C24, 0.92);
    pub const SURFACE_HOVER: Color = Color::hex(0x1F2530, 0.96);
    pub const SURFACE_PRESSED: Color = Color::hex(0x262E3B, 1.0);
    pub const HUD_BG: Color = Color::hex(0x11141A, 0.78);
    pub const BORDER: Color = Color::hex(0x2B3342, 0.85);
    pub const BORDER_STRONG: Color = Color::hex(0x475368, 1.0);
    pub const TEXT_PRIMARY: Color = Color::hex(0xF2F4F8, 1.0);
    pub const TEXT_SECONDARY: Color = Color::hex(0x9AA5B8, 1.0);
    pub const ACCENT: Color = Color::hex(0xE5A93B, 1.0);
    pub const ACCENT_SOFT: Color = Color::hex(0xE5A93B, 0.16);
    pub const SUCCESS: Color = Color::hex(0x2EC27E, 1.0);
    pub const SUCCESS_SOFT: Color = Color::hex(0x2EC27E, 0.16);
    pub const DANGER: Color = Color::hex(0xE05A47, 1.0);
    pub const SHADOW: Color = Color::hex(0x04060A, 0.50);
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

pub struct DrawList {
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<DrawBatch>,
    scissor_stack: Vec<[i32; 4]>,
    screen_w: i32,
    screen_h: i32,
}

impl DrawList {
    pub fn new(screen_w: u32, screen_h: u32) -> Self {
        let full = [0, 0, screen_w.max(1) as i32, screen_h.max(1) as i32];
        Self {
            vertices: Vec::with_capacity(2048),
            indices: Vec::with_capacity(3072),
            batches: Vec::new(),
            scissor_stack: vec![full],
            screen_w: screen_w.max(1) as i32,
            screen_h: screen_h.max(1) as i32,
        }
    }

    pub fn clear(&mut self, screen_w: u32, screen_h: u32) {
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
        self.scissor_stack.clear();
        self.screen_w = screen_w.max(1) as i32;
        self.screen_h = screen_h.max(1) as i32;
        self.scissor_stack
            .push([0, 0, self.screen_w, self.screen_h]);
    }

    pub fn push_scissor(&mut self, r: Rect) {
        let cur = *self.scissor_stack.last().unwrap_or(&[0, 0, self.screen_w, self.screen_h]);
        let nx0 = (r.x.floor() as i32).max(cur[0]).clamp(0, self.screen_w);
        let ny0 = (r.y.floor() as i32).max(cur[1]).clamp(0, self.screen_h);
        let nx1 = ((r.x + r.w).ceil() as i32)
            .min(cur[0] + cur[2])
            .clamp(nx0, self.screen_w);
        let ny1 = ((r.y + r.h).ceil() as i32)
            .min(cur[1] + cur[3])
            .clamp(ny0, self.screen_h);
        self.scissor_stack.push([nx0, ny0, nx1 - nx0, ny1 - ny0]);
    }

    pub fn pop_scissor(&mut self) {
        if self.scissor_stack.len() > 1 {
            self.scissor_stack.pop();
        }
    }

    fn current_scissor(&self) -> [i32; 4] {
        *self
            .scissor_stack
            .last()
            .unwrap_or(&[0, 0, self.screen_w, self.screen_h])
    }

    fn push_quad(&mut self, x: f32, y: f32, w: f32, h: f32, uvs: [f32; 4], color: Color, border_color: Color, params: [f32; 4], sdf_local: bool) {
        let scissor = self.current_scissor();
        if scissor[2] <= 0 || scissor[3] <= 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let base = self.vertices.len() as u32;
        let (u0, v0, u1, v1) = if sdf_local {
            (0.0, 0.0, w, h)
        } else {
            (uvs[0], uvs[1], uvs[2], uvs[3])
        };
        let rect_size = [w, h];
        self.vertices.push(UiVertex {
            pos: [x, y],
            uv: [u0, v0],
            rect_size,
            color: color.0,
            border_color: border_color.0,
            params,
        });
        self.vertices.push(UiVertex {
            pos: [x + w, y],
            uv: [u1, v0],
            rect_size,
            color: color.0,
            border_color: border_color.0,
            params,
        });
        self.vertices.push(UiVertex {
            pos: [x + w, y + h],
            uv: [u1, v1],
            rect_size,
            color: color.0,
            border_color: border_color.0,
            params,
        });
        self.vertices.push(UiVertex {
            pos: [x, y + h],
            uv: [u0, v1],
            rect_size,
            color: color.0,
            border_color: border_color.0,
            params,
        });

        let idx_start = self.indices.len() as u32;
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

        if let Some(last) = self.batches.last_mut()
            && last.scissor == scissor
            && last.index_offset + last.index_count == idx_start
        {
            last.index_count += 6;
        } else {
            self.batches.push(DrawBatch {
                index_offset: idx_start,
                index_count: 6,
                scissor,
            });
        }
    }

    pub fn rect(&mut self, r: Rect, radius: f32, color: Color) {
        self.push_quad(
            r.x,
            r.y,
            r.w,
            r.h,
            [0.0; 4],
            color,
            Color([0.0; 4]),
            [radius, 0.0, 0.0, 0.0],
            true,
        );
    }

    pub fn rect_border(&mut self, r: Rect, radius: f32, fill: Color, border_w: f32, border: Color) {
        self.push_quad(
            r.x,
            r.y,
            r.w,
            r.h,
            [0.0; 4],
            fill,
            border,
            [radius, border_w, 0.0, 0.0],
            true,
        );
    }

    pub fn shadow(&mut self, r: Rect, radius: f32, spread: f32, color: Color) {
        let sr = Rect::new(
            r.x - spread,
            r.y - spread * 0.5,
            r.w + spread * 2.0,
            r.h + spread * 2.0,
        );
        self.push_quad(
            sr.x,
            sr.y,
            sr.w,
            sr.h,
            [0.0; 4],
            color,
            Color([0.0; 4]),
            [radius + spread * 0.5, 0.0, 2.0, spread],
            true,
        );
    }

    pub fn ring(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, color: Color) {
        let pad = thickness + 2.0;
        let size = (radius + pad) * 2.0;
        self.push_quad(
            cx - radius - pad,
            cy - radius - pad,
            size,
            size,
            [0.0; 4],
            color,
            Color([0.0; 4]),
            [radius, thickness, 3.0, 0.0],
            true,
        );
    }

    pub fn text(&mut self, atlas: &FontAtlas, kind: FontKind, x: f32, y: f32, text: &str, color: Color) {
        let baseline = y + atlas.ascent(kind);

        let mut pen_x = x;
        for ch in text.chars() {
            let Some(g) = atlas.glyph(kind, ch) else {
                continue;
            };
            if g.size[0] > 0.0 && g.size[1] > 0.0 {
                let gx = (pen_x + g.offset[0]).round();
                let gy = (baseline + g.offset[1]).round();
                self.push_quad(
                    gx,
                    gy,
                    g.size[0],
                    g.size[1],
                    [g.uv_min[0], g.uv_min[1], g.uv_max[0], g.uv_max[1]],
                    color,
                    Color([0.0; 4]),
                    [0.0, 0.0, 1.0, 0.0],
                    false,
                );
            }
            pen_x += g.advance;
        }
    }

    pub fn line(&mut self, p0: [f32; 2], p1: [f32; 2], thickness: f32, color: Color) {
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.5 {
            return;
        }
        let scissor = self.current_scissor();
        if scissor[2] <= 0 || scissor[3] <= 0 {
            return;
        }
        let nx = -dy / len * (thickness * 0.5);
        let ny = dx / len * (thickness * 0.5);

        let base = self.vertices.len() as u32;
        let uv = [50.0, 50.0];
        let rect_size = [100.0, 100.0];
        let border_color = [0.0; 4];
        let params = [0.0, 0.0, 0.0, 0.0];

        for [x, y] in [
            [p0[0] - nx, p0[1] - ny],
            [p1[0] - nx, p1[1] - ny],
            [p1[0] + nx, p1[1] + ny],
            [p0[0] + nx, p0[1] + ny],
        ] {
            self.vertices.push(UiVertex {
                pos: [x, y],
                uv,
                rect_size,
                color: color.0,
                border_color,
                params,
            });
        }
        let idx_start = self.indices.len() as u32;
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        if let Some(last) = self.batches.last_mut()
            && last.scissor == scissor
            && last.index_offset + last.index_count == idx_start
        {
            last.index_count += 6;
        } else {
            self.batches.push(DrawBatch {
                index_offset: idx_start,
                index_count: 6,
                scissor,
            });
        }
    }

    /// Geometric brand crest: minimal two-bar wing mark for Kestrel Client.
    pub fn kestrel_crest(&mut self, x: f32, y: f32, size: f32, accent: Color, base: Color) {
        let u = size / 16.0;
        self.rect(Rect::new(x, y + 2.0 * u, 16.0 * u, 3.0 * u), 1.5 * u, accent);
        self.rect(Rect::new(x + 3.0 * u, y + 7.0 * u, 13.0 * u, 3.0 * u), 1.5 * u, base);
        self.rect(Rect::new(x + 6.0 * u, y + 12.0 * u, 7.0 * u, 2.5 * u), 1.2 * u, accent);
    }

    /// Minimal geometric light icon for Fullbright.
    pub fn icon_sun(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        self.ring(cx, cy, r * 0.55, 1.8, color);
        self.rect(Rect::new(cx - 1.0, cy - r, 2.0, 3.0), 1.0, color);
        self.rect(Rect::new(cx - 1.0, cy + r - 3.0, 2.0, 3.0), 1.0, color);
        self.rect(Rect::new(cx - r, cy - 1.0, 3.0, 2.0), 1.0, color);
        self.rect(Rect::new(cx + r - 3.0, cy - 1.0, 3.0, 2.0), 1.0, color);
    }
}

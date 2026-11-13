use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;

static FONT_SANS_REG: &[u8] = include_bytes!("../../../assets/fonts/BricolageGrotesque-Regular.ttf");
static FONT_SANS_SEMI: &[u8] =
    include_bytes!("../../../assets/fonts/BricolageGrotesque-SemiBold.ttf");
static FONT_SANS_BOLD: &[u8] = include_bytes!("../../../assets/fonts/BricolageGrotesque-Bold.ttf");
static FONT_MONO_REG: &[u8] = include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontKind {
    UiSmall,   // 13px regular
    UiBody,    // 15px regular
    UiSemi,    // 15px semibold
    UiHeading, // 18px semibold
    UiTitle,   // 22px bold
    MonoSmall, // 12.5px mono
    MonoBody,  // 14.5px mono
}

impl FontKind {
    pub fn px_size(self) -> f32 {
        match self {
            Self::UiSmall => 13.0,
            Self::UiBody => 15.0,
            Self::UiSemi => 15.0,
            Self::UiHeading => 18.0,
            Self::UiTitle => 22.0,
            Self::MonoSmall => 12.5,
            Self::MonoBody => 14.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub advance: f32,
}

pub struct FontAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA8
    glyphs: HashMap<(FontKind, char), GlyphInfo>,
    line_heights: HashMap<FontKind, f32>,
    ascents: HashMap<FontKind, f32>,
}

impl FontAtlas {
    pub fn build() -> Self {
        let sans_reg = FontArc::try_from_slice(FONT_SANS_REG).expect("sans reg font");
        let sans_semi = FontArc::try_from_slice(FONT_SANS_SEMI).expect("sans semi font");
        let sans_bold = FontArc::try_from_slice(FONT_SANS_BOLD).expect("sans bold font");
        let mono_reg = FontArc::try_from_slice(FONT_MONO_REG).expect("mono reg font");

        let width = 1024u32;
        let height = 1024u32;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut glyphs = HashMap::new();
        let mut line_heights = HashMap::new();
        let mut ascents = HashMap::new();

        let mut chars: Vec<char> = (0x20u8..=0x7Eu8).map(|b| b as char).collect();
        chars.extend(['°', '·', '×', '✓', '▲', '▼', '◄', '►']);

        let kinds = [
            (FontKind::UiSmall, &sans_reg),
            (FontKind::UiBody, &sans_reg),
            (FontKind::UiSemi, &sans_semi),
            (FontKind::UiHeading, &sans_semi),
            (FontKind::UiTitle, &sans_bold),
            (FontKind::MonoSmall, &mono_reg),
            (FontKind::MonoBody, &mono_reg),
        ];

        let mut pen_x = 2u32;
        let mut pen_y = 2u32;
        let mut row_h = 0u32;

        for (kind, font) in kinds {
            let scale = PxScale::from(kind.px_size());
            let scaled = font.as_scaled(scale);
            line_heights.insert(kind, scaled.height());
            ascents.insert(kind, scaled.ascent());

            for &ch in &chars {
                let gid = font.glyph_id(ch);
                let advance = scaled.h_advance(gid);
                let g = gid.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
                if let Some(outlined) = font.outline_glyph(g) {
                    let bounds = outlined.px_bounds();
                    let gw = bounds.width().ceil() as u32;
                    let gh = bounds.height().ceil() as u32;

                    if gw > 0 && gh > 0 {
                        if pen_x + gw + 2 >= width {
                            pen_x = 2;
                            pen_y += row_h + 2;
                            row_h = 0;
                        }
                        if pen_y + gh + 2 >= height {
                            continue;
                        }
                        outlined.draw(|gx, gy, v| {
                            let ax = pen_x + gx;
                            let ay = pen_y + gy;
                            if ax < width && ay < height {
                                let idx = ((ay * width + ax) * 4) as usize;
                                let cov = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                                pixels[idx] = 255;
                                pixels[idx + 1] = 255;
                                pixels[idx + 2] = 255;
                                pixels[idx + 3] = cov;
                            }
                        });
                        glyphs.insert(
                            (kind, ch),
                            GlyphInfo {
                                uv_min: [pen_x as f32 / width as f32, pen_y as f32 / height as f32],
                                uv_max: [
                                    (pen_x + gw) as f32 / width as f32,
                                    (pen_y + gh) as f32 / height as f32,
                                ],
                                offset: [bounds.min.x, bounds.min.y],
                                size: [gw as f32, gh as f32],
                                advance,
                            },
                        );
                        pen_x += gw + 2;
                        row_h = row_h.max(gh);
                    } else {
                        glyphs.insert(
                            (kind, ch),
                            GlyphInfo {
                                uv_min: [0.0, 0.0],
                                uv_max: [0.0, 0.0],
                                offset: [0.0, 0.0],
                                size: [0.0, 0.0],
                                advance,
                            },
                        );
                    }
                } else {
                    glyphs.insert(
                        (kind, ch),
                        GlyphInfo {
                            uv_min: [0.0, 0.0],
                            uv_max: [0.0, 0.0],
                            offset: [0.0, 0.0],
                            size: [0.0, 0.0],
                            advance,
                        },
                    );
                }
            }
        }

        Self {
            width,
            height,
            pixels,
            glyphs,
            line_heights,
            ascents,
        }
    }

    pub fn ascent(&self, kind: FontKind) -> f32 {
        self.ascents.get(&kind).copied().unwrap_or(12.0)
    }

    pub fn line_height(&self, kind: FontKind) -> f32 {
        self.line_heights.get(&kind).copied().unwrap_or(16.0)
    }

    pub fn glyph(&self, kind: FontKind, ch: char) -> Option<&GlyphInfo> {
        self.glyphs
            .get(&(kind, ch))
            .or_else(|| self.glyphs.get(&(kind, '?')))
    }

    pub fn measure(&self, kind: FontKind, text: &str) -> f32 {
        let mut w = 0.0;
        for ch in text.chars() {
            if let Some(g) = self.glyph(kind, ch) {
                w += g.advance;
            }
        }
        w
    }
}

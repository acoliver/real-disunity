use super::*;
use crate::components::Component;
use ab_glyph::{Font, Glyph, GlyphId, OutlinedGlyph, ScaleFont, point};
use fields::Text;
use glam::Vec2;

struct Line {
    characters: Vec<char>,
    width: f32,
}
struct TextGeometry<'a> {
    font: &'a FontArc,
    text: &'a Text,
    clip: [f32; 4],
}
impl DisunityWorld {
    pub(super) fn text_snapshot(
        &self,
        c: &Component,
        t: &Text,
        pixels: &mut usize,
        glyph_count: &mut usize,
    ) -> Result<Value, String> {
        let (_, canvas) = self
            .canvas(c.object)
            .ok_or("Text requires a Canvas ancestor")?;
        let rect = self.rect_snapshot(c.object)?;
        let clip: [f32; 4] =
            serde_json::from_value(rect["local_rect"].clone()).map_err(|e| e.to_string())?;
        let mut glyphs = Vec::new();
        if !t.text.is_empty() {
            let font = &self
                .ui
                .fonts
                .get(&t.font)
                .ok_or_else(|| format!("font is not registered: {}", t.font))?
                .0;
            let geometry = TextGeometry {
                font,
                text: t,
                clip,
            };
            for (ch, glyph) in geometry.positioned()? {
                if let Some(outlined) = font.outline_glyph(glyph)
                    && let Some(mut g) = geometry.raster(ch, outlined, pixels)?
                {
                    *glyph_count += 1;
                    if *glyph_count > 4096 {
                        return Err("UI glyph budget exceeded".into());
                    }
                    let r = serde_json::from_value(g["rect"].clone()).map_err(|e| e.to_string())?;
                    let corners = self.rect_corners(c.object, r)?;
                    g["world_corners"] = json!(corners.world);
                    g["screen_corners"] = json!(corners.screen);
                    glyphs.push(g);
                }
            }
        }
        Ok(
            json!({"object":c.object,"component":c.handle,"text":t.text,"color":t.color,"font":t.font,"font_size":t.font_size,"sorting_layer":canvas.sorting_layer,"sorting_order":canvas.sorting_order,"rect":rect,"glyphs":glyphs}),
        )
    }
}
impl TextGeometry<'_> {
    fn lines(&self) -> Result<Vec<Line>, String> {
        let font = self.font.as_scaled(self.text.font_size);
        let mut lines = Vec::new();
        let mut line = Line {
            characters: Vec::new(),
            width: 0.0,
        };
        let mut previous = None;
        for ch in self.text.text.chars() {
            if ch == '\r' {
                continue;
            }
            if ch == '\n' {
                lines.push(line);
                line = Line {
                    characters: Vec::new(),
                    width: 0.0,
                };
                previous = None;
                continue;
            }
            let id = font.glyph_id(ch);
            if id == GlyphId(0) {
                return Err(format!("font has no glyph for U+{:04X}", ch as u32));
            }
            let kern = previous.map_or(0.0, |p| font.kern(p, id));
            let advance = font.h_advance(id);
            if self.text.wrap
                && !line.characters.is_empty()
                && line.width + kern + advance > self.clip[2] - self.clip[0]
            {
                lines.push(line);
                line = Line {
                    characters: Vec::new(),
                    width: 0.0,
                };
                previous = None;
            }
            line.width += previous.map_or(0.0, |p| font.kern(p, id)) + advance;
            line.characters.push(ch);
            previous = Some(id);
        }
        lines.push(line);
        Ok(lines)
    }
    fn positioned(&self) -> Result<Vec<(char, Glyph)>, String> {
        let lines = self.lines()?;
        let font = self.font.as_scaled(self.text.font_size);
        let spacing = (font.height() + font.line_gap()) * self.text.line_spacing;
        let height = font.height() + spacing * (lines.len().saturating_sub(1)) as f32;
        let row = f32::from(self.text.alignment / 3) * 0.5;
        let column = f32::from(self.text.alignment % 3) * 0.5;
        let top = self.clip[3] - (self.clip[3] - self.clip[1] - height) * row;
        let mut out = Vec::new();
        for (i, line) in lines.into_iter().enumerate() {
            let mut x = self.clip[0] + (self.clip[2] - self.clip[0] - line.width) * column;
            let baseline = top - font.ascent() - i as f32 * spacing;
            let mut previous = None;
            for ch in line.characters {
                let id = font.glyph_id(ch);
                x += previous.map_or(0.0, |p| font.kern(p, id));
                out.push((
                    ch,
                    id.with_scale_and_position(self.text.font_size, point(x, -baseline)),
                ));
                x += font.h_advance(id);
                previous = Some(id);
            }
        }
        Ok(out)
    }
    fn raster(
        &self,
        ch: char,
        glyph: OutlinedGlyph,
        pixels: &mut usize,
    ) -> Result<Option<Value>, String> {
        let bounds = glyph.px_bounds();
        let mut min = Vec2::new(bounds.min.x, bounds.min.y);
        let mut max = Vec2::new(bounds.max.x, bounds.max.y);
        if !self.text.overflow {
            min = min.max(Vec2::new(self.clip[0].ceil(), (-self.clip[3]).ceil()));
            max = max.min(Vec2::new(self.clip[2].floor(), (-self.clip[1]).floor()));
        }
        if max.x <= min.x || max.y <= min.y {
            return Ok(None);
        }
        let width = (max.x - min.x) as usize;
        let height = (max.y - min.y) as usize;
        let count = width
            .checked_mul(height)
            .ok_or("glyph bitmap size overflow")?;
        *pixels = pixels.checked_add(count).ok_or("UI bitmap size overflow")?;
        if *pixels > 4 * 1024 * 1024 {
            return Err("UI bitmap budget exceeded".into());
        }
        let mut alpha = vec![0_u8; count];
        glyph.draw(|x, y, coverage| {
            let x = x as i64 + (bounds.min.x - min.x) as i64;
            let y = y as i64 + (bounds.min.y - min.y) as i64;
            if x >= 0 && y >= 0 && x < width as i64 && y < height as i64 {
                alpha[y as usize * width + x as usize] = (coverage * 255.0).round() as u8;
            }
        });
        Ok(Some(
            json!({"character":ch.to_string(),"glyph_id":glyph.glyph().id.0,"rect":[min.x,-max.y,max.x,-min.y],"width":width,"height":height,"alpha_base64":STANDARD.encode(alpha)}),
        ))
    }
}

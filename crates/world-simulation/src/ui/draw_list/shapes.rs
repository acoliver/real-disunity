use super::{UiBorder, UiDrawKind, UiDrawList, UiError, UiRect, check_color};

impl UiDrawList<'_> {
    pub fn solid_rect(&mut self, rect: UiRect, color: [f32; 4]) -> Result<(), UiError> {
        rect.validate()?;
        check_color(color)?;
        if self.hidden(rect) {
            return Ok(());
        }
        self.check_count(1)?;
        self.commit(vec![self.quad(
            rect.points(),
            [0., 0., 1., 1.],
            color,
            None,
            UiDrawKind::Graphic,
        )?])
    }
    pub fn border_rect(&mut self, rect: UiRect, style: UiBorder) -> Result<(), UiError> {
        rect.validate()?;
        check_color(style.color)?;
        if let Some(fill) = style.fill {
            check_color(fill)?;
        }
        if !style.width.is_finite() || style.width.min_element() < 0. {
            return Err(UiError::InvalidRect);
        }
        if self.hidden(rect) {
            return Ok(());
        }
        let width = style.width.min(rect.size * 0.5);
        let (x, y, w, h) = (rect.origin.x, rect.origin.y, rect.size.x, rect.size.y);
        let (dx, dy) = (width.x, width.y);
        let inner = UiRect::new(x + dx, y + dy, w - 2. * dx, h - 2. * dy);
        let mut regions = Vec::with_capacity(5);
        if let Some(fill) = style.fill {
            regions.push((inner, fill));
        }
        regions.extend([
            (UiRect::new(x, y, w, dy), style.color),
            (UiRect::new(x, y + h - dy, w, dy), style.color),
            (UiRect::new(x, y + dy, dx, inner.size.y), style.color),
            (
                UiRect::new(x + w - dx, y + dy, dx, inner.size.y),
                style.color,
            ),
        ]);
        regions.retain(|(rect, _)| !rect.empty());
        self.check_count(regions.len())?;
        let draws = regions
            .into_iter()
            .map(|(r, c)| self.quad(r.points(), [0., 0., 1., 1.], c, None, UiDrawKind::Graphic))
            .collect::<Result<_, _>>()?;
        self.commit(draws)
    }
}

use super::{
    UiDrawKind, UiDrawList, UiError, UiImage, UiImageOptions, UiRect, UiTexture, check_color,
    check_transform, rotate,
};

impl UiDrawList<'_> {
    pub fn image(
        &mut self,
        image: UiImage,
        destination: UiRect,
        options: UiImageOptions,
    ) -> Result<(), UiError> {
        destination.validate()?;
        let source = image.source(options.source)?;
        for color in options.colors {
            check_color(color)?;
        }
        check_transform(options.rotation_radians, options.pivot)?;
        if self.hidden(destination) {
            return Ok(());
        }
        self.check_count(1)?;
        let size = image.size.as_vec2();
        let (low, high) = (source.origin / size, (source.origin + source.size) / size);
        let mut uv = [low.x, low.y, high.x, high.y];
        if options.flip.x() {
            uv.swap(0, 2);
        }
        if options.flip.y() {
            uv.swap(1, 3);
        }
        let points = rotate(
            destination.points(),
            destination,
            options.rotation_radians,
            options.pivot,
        );
        let mut draw = self.quad(
            points,
            uv,
            [1.; 4],
            Some(UiTexture::Image(image.id.0)),
            UiDrawKind::Graphic,
        )?;
        draw.corner_colors = Some(options.colors);
        self.commit(vec![draw])
    }
}

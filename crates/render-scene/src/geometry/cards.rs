use super::GeometryError;
use worldrunner_render_plan::{SceneMeshLod, SceneVertex};

#[derive(Clone, Copy)]
struct CardShape {
    width: i128,
    height: i128,
    radius: i128,
    bottom: i128,
    span: i128,
}

impl CardShape {
    fn new(lod: &SceneMeshLod) -> Result<Option<Self>, GeometryError> {
        let Some(card) = lod.card() else {
            return Ok(None);
        };
        let mut radius = 0;
        let mut bottom = i128::MAX;
        let mut top = i128::MIN;
        for vertex in lod.vertices() {
            let [x, y, z] = vertex.position_millimeters.map(i128::from);
            radius = radius.max(x.abs()).max(y.abs());
            bottom = bottom.min(z);
            top = top.max(z);
        }
        if radius == 0 || top == bottom {
            return Err(GeometryError::DegenerateCardExtent);
        }
        let [width, height] = card.size_millimeters().map(i128::from);
        Ok(Some(Self {
            width,
            height,
            radius,
            bottom,
            span: top - bottom,
        }))
    }

    fn vertex(self, mut vertex: SceneVertex) -> SceneVertex {
        let [x, y, z] = vertex.position_millimeters.map(i128::from);
        let scaled = [
            x * self.width / (2 * self.radius),
            y * self.width / (2 * self.radius),
            (z - self.bottom) * self.height / self.span,
        ];
        vertex.position_millimeters = scaled
            .map(|value| i64::try_from(value).expect("scaled card fits authored u32 dimensions"));
        vertex
    }
}

/// Fit centered horizontal extents to width and the foot-to-top span to height.
/// Both crossed planes retain their authored orientation and texture coordinates.
pub(crate) fn card_vertices(
    lod: &SceneMeshLod,
) -> Result<impl Iterator<Item = SceneVertex> + '_, GeometryError> {
    let shape = CardShape::new(lod)?;
    Ok(lod
        .vertices()
        .iter()
        .map(move |vertex| shape.map_or(*vertex, |shape| shape.vertex(*vertex))))
}

/// Inverse transpose of the dimension fit, before placement and normalization.
#[expect(
    clippy::cast_precision_loss,
    reason = "card integer extents are converted once for GPU normals"
)]
pub(super) fn normal_scale(lod: &SceneMeshLod) -> Result<glam::Vec3, GeometryError> {
    let Some(shape) = CardShape::new(lod)? else {
        return Ok(glam::Vec3::ONE);
    };
    let horizontal = super::narrow((2 * shape.radius) as f64 / shape.width as f64);
    let vertical = super::narrow(shape.span as f64 / shape.height as f64);
    Ok(glam::Vec3::new(horizontal, horizontal, vertical))
}

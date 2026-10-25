use super::{Canvas, CanvasMode, RectTransform, UiError};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};
use glam::{Mat4, UVec2, Vec2, Vec3};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct RectLayout {
    pub object: ObjectHandle,
    pub canvas: ObjectHandle,
    pub size: Vec2,
    pub pivot: Vec2,
    pub local_to_world: Mat4,
    pub corners: [Vec3; 4],
    pub screen_corners: Option<[Vec2; 4]>,
    pub sorting_layer: i32,
    pub sorting_order: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiLayout {
    pub viewport: UVec2,
    pub rects: Vec<RectLayout>,
}
impl UiLayout {
    pub fn rect(&self, object: ObjectHandle) -> Option<&RectLayout> {
        self.rects.iter().find(|r| r.object == object)
    }
}

#[derive(Clone, Copy)]
struct Resolved {
    size: Vec2,
    pivot: Vec2,
    matrix: Mat4,
    canvas: Option<(ObjectHandle, Canvas)>,
    visible: bool,
}

impl World {
    /// Resolves completed live state without mutating serialized local transforms.
    pub fn ui_layout(&self, viewport: UVec2) -> Result<UiLayout, WorldError> {
        if viewport.min_element() == 0 || viewport.max_element() > 16384 {
            return Err(UiError::InvalidViewport.into());
        }
        let mut rects = BTreeMap::new();
        let mut canvases = BTreeMap::new();
        for (_, c) in self.components.iter(self.identity) {
            if self.lifetime.components.contains(&c.handle)
                || self.lifetime.objects.contains(&c.object)
            {
                continue;
            }
            match c.desc {
                ComponentDesc::RectTransform(r) => {
                    rects.insert(c.object, r);
                }
                ComponentDesc::Canvas(ca) => {
                    canvases.insert(c.object, ca);
                }
                _ => {}
            }
        }
        if canvases.keys().any(|o| !rects.contains_key(o)) {
            return Err(UiError::MissingLayout.into());
        }
        let mut resolved = BTreeMap::new();
        for &object in rects.keys() {
            let mut chain = Vec::new();
            let mut current = Some(object);
            while let Some(node) = current {
                if resolved.contains_key(&node) {
                    break;
                }
                chain.push(node);
                current = self.parent(node)?;
            }
            for node in chain.into_iter().rev() {
                let parent = self.parent(node)?.and_then(|p| resolved.get(&p).copied());
                let value = self.resolve_ui_node(node, parent, &rects, &canvases, viewport)?;
                resolved.insert(node, value);
            }
        }
        Ok(UiLayout {
            viewport,
            rects: self.ui_rects(&rects, &canvases, &resolved, viewport)?,
        })
    }

    fn ui_rects(
        &self,
        rects: &BTreeMap<ObjectHandle, RectTransform>,
        canvases: &BTreeMap<ObjectHandle, Canvas>,
        resolved: &BTreeMap<ObjectHandle, Resolved>,
        viewport: UVec2,
    ) -> Result<Vec<RectLayout>, WorldError> {
        let mut output = Vec::new();
        for &object in rects.keys() {
            let r = resolved[&object];
            let Some((canvas, settings)) = r.canvas else {
                continue;
            };
            if !r.visible {
                continue;
            }
            let min = -r.pivot * r.size;
            let max = min + r.size;
            let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
                .map(|p| r.matrix.transform_point3(p.extend(0.)));
            if corners.iter().any(|p| !p.is_finite()) {
                return Err(UiError::InvalidRect.into());
            }
            let screen_corners = match settings.mode {
                CanvasMode::ScreenSpaceOverlay => Some(corners.map(Vec3::truncate)),
                CanvasMode::ScreenSpaceCamera {
                    camera,
                    orthographic_size,
                    ..
                } => {
                    let inverse = self.ui_camera(camera, canvases)?.inverse();
                    let pixels_per_unit =
                        f32::from(u16::try_from(viewport.y).expect("bounded viewport"))
                            / (2. * orthographic_size);
                    Some(corners.map(|p| {
                        inverse.transform_point3(p).truncate() * pixels_per_unit
                            + viewport.as_vec2() * 0.5
                    }))
                }
                CanvasMode::WorldSpace => None,
            };
            output.push(RectLayout {
                object,
                canvas,
                size: r.size,
                pivot: r.pivot,
                local_to_world: r.matrix,
                corners,
                screen_corners,
                sorting_layer: settings.sorting_layer,
                sorting_order: settings.sorting_order,
            });
        }
        Ok(output)
    }

    fn resolve_ui_node(
        &self,
        object: ObjectHandle,
        parent: Option<Resolved>,
        rects: &BTreeMap<ObjectHandle, RectTransform>,
        canvases: &BTreeMap<ObjectHandle, Canvas>,
        viewport: UVec2,
    ) -> Result<Resolved, WorldError> {
        let rect = rects.get(&object);
        let canvas = canvases.get(&object).copied();
        let mut local = self.local_transform(object)?;
        let mut size = Vec2::ZERO;
        let pivot = rect.map_or(Vec2::ZERO, |r| r.pivot);
        if let Some(r) = rect {
            let parent_size = parent.map_or(Vec2::ZERO, |p| p.size);
            size = r.size_delta + parent_size * (r.anchor_max - r.anchor_min);
            let anchor = r.anchor_min + (r.anchor_max - r.anchor_min) * pivot;
            let xy =
                r.anchored_position + parent.map_or(Vec2::ZERO, |p| p.size * (anchor - p.pivot));
            local.translation.x = xy.x;
            local.translation.y = xy.y;
        }
        let mut matrix = parent.map_or(Mat4::IDENTITY, |p| p.matrix) * local.matrix();
        if let Some(c) = canvas {
            if rect.is_none() {
                return Err(UiError::MissingLayout.into());
            }
            if c.mode != CanvasMode::WorldSpace {
                if self.parent(object)?.is_some() {
                    return Err(UiError::ScreenCanvasParent.into());
                }
                let factor = c.scale.factor(viewport)?;
                size = viewport.as_vec2() / factor;
                matrix = self.screen_matrix(c.mode, pivot, viewport, factor, canvases)?;
            }
        }
        if !size.is_finite() || size.min_element() < 0. || !matrix.is_finite() {
            return Err(UiError::InvalidRect.into());
        }
        Ok(Resolved {
            size,
            pivot,
            matrix,
            canvas: canvas
                .map(|c| (object, c))
                .or_else(|| parent.and_then(|p| p.canvas)),
            visible: self.live_object(object)?.active
                && parent.is_none_or(|p| p.visible)
                && canvas.is_none_or(|c| c.enabled),
        })
    }

    fn screen_matrix(
        &self,
        mode: CanvasMode,
        pivot: Vec2,
        viewport: UVec2,
        factor: f32,
        canvases: &BTreeMap<ObjectHandle, Canvas>,
    ) -> Result<Mat4, WorldError> {
        Ok(match mode {
            CanvasMode::ScreenSpaceOverlay => Mat4::from_scale_rotation_translation(
                Vec3::splat(factor),
                glam::Quat::IDENTITY,
                (pivot * viewport.as_vec2()).extend(0.),
            ),
            CanvasMode::ScreenSpaceCamera {
                camera,
                orthographic_size,
                plane_distance,
            } => {
                let unit = 2. * orthographic_size
                    / f32::from(u16::try_from(viewport.y).expect("bounded viewport"));
                let xy = (pivot - Vec2::splat(0.5)) * viewport.as_vec2() * unit;
                self.ui_camera(camera, canvases)?
                    * Mat4::from_translation(xy.extend(plane_distance))
                    * Mat4::from_scale(Vec3::splat(unit * factor))
            }
            CanvasMode::WorldSpace => unreachable!(),
        })
    }

    fn ui_camera(
        &self,
        camera: ObjectHandle,
        canvases: &BTreeMap<ObjectHandle, Canvas>,
    ) -> Result<Mat4, WorldError> {
        let mut current = Some(camera);
        while let Some(node) = current {
            if canvases.contains_key(&node) {
                return Err(UiError::InvalidCamera.into());
            }
            current = self.parent(node)?;
        }
        let matrix = self.world_matrix(camera)?;
        let rigid =
            Mat4::from_rotation_translation(self.world_rotation(camera)?, matrix.w_axis.truncate());
        if !matrix.is_finite() || !matrix.abs_diff_eq(rigid, 1e-4) {
            return Err(UiError::InvalidCamera.into());
        }
        Ok(matrix)
    }
}

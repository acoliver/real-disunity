use super::*;
use crate::{
    DisunityObject,
    components::{Camera, Component},
};
use fields::{Canvas, RectTransform, RenderMode};
use glam::{Quat, Vec2, Vec3};

type Trs = (Vec3, Quat, Vec3);
pub(super) struct Corners {
    pub world: Vec<[f32; 3]>,
    pub screen: Option<Vec<[f32; 2]>>,
}
impl DisunityWorld {
    pub(crate) fn set_rect_position(
        &mut self,
        object: u64,
        position: [f32; 3],
    ) -> Result<(), String> {
        let Some((c, r)) = self.rect_fields(object) else {
            return Ok(());
        };
        let component = self.component_index(c.handle)?;
        let mut anchored = Vec2::new(position[0], position[1]);
        if let Some(parent) = self.metadata[&object].parent
            && let Some((_, pr)) = self.rect_fields(parent)
        {
            let anchor = Vec2::from(r.anchor_min)
                + (Vec2::from(r.anchor_max) - Vec2::from(r.anchor_min)) * Vec2::from(r.pivot);
            anchored -= self.rect_size(parent)? * (anchor - Vec2::from(pr.pivot));
        }
        if let Fields::Rect(r) = &mut self.components[component].as_mut().unwrap().fields {
            r.anchored_position = anchored.to_array();
        }
        Ok(())
    }
    fn rect_fields(&self, object: u64) -> Option<(&Component, &RectTransform)> {
        self.components
            .iter()
            .flatten()
            .find_map(|c| match &c.fields {
                Fields::Rect(r) if c.object == object => Some((c, r)),
                _ => None,
            })
    }
    pub(super) fn canvas(&self, object: u64) -> Option<(u64, &Canvas)> {
        let mut current = Some(object);
        while let Some(h) = current {
            if let Some(canvas) = self
                .components
                .iter()
                .flatten()
                .find_map(|c| match &c.fields {
                    Fields::Canvas(v) if c.object == h => Some(v),
                    _ => None,
                })
            {
                return Some((h, canvas));
            }
            current = self.metadata.get(&h)?.parent;
        }
        None
    }
    fn root_canvas(&self, object: u64) -> Option<&Canvas> {
        self.canvas(object)
            .filter(|(h, _)| *h == object)
            .map(|(_, c)| c)
    }
    fn canvas_factor(&self, object: u64) -> f32 {
        self.components
            .iter()
            .flatten()
            .find_map(|c| match &c.fields {
                Fields::CanvasScaler(v) if c.object == object && c.enabled => {
                    Some(v.factor(self.ui.viewport))
                }
                _ => None,
            })
            .unwrap_or(1.0)
    }
    pub(super) fn rect_size(&self, object: u64) -> Result<Vec2, String> {
        let (_, r) = self
            .rect_fields(object)
            .ok_or("object requires RectTransform")?;
        if self
            .root_canvas(object)
            .is_some_and(|c| c.render_mode != RenderMode::WorldSpace)
        {
            return Ok(
                Vec2::new(self.ui.viewport[0] as f32, self.ui.viewport[1] as f32)
                    / self.canvas_factor(object),
            );
        }
        let parent = self.metadata[&object].parent;
        let parent_size = parent
            .filter(|p| self.rect_fields(*p).is_some())
            .map(|p| self.rect_size(p))
            .transpose()?
            .unwrap_or(Vec2::ZERO);
        let size = Vec2::from(r.size_delta)
            + parent_size * (Vec2::from(r.anchor_max) - Vec2::from(r.anchor_min));
        if !size.is_finite() || size.min_element() < 0.0 {
            return Err("resolved rectangle has invalid size".into());
        }
        Ok(size)
    }
    pub(crate) fn local_trs(&self, object: u64, o: &DisunityObject) -> Result<Trs, String> {
        let Some((_, r)) = self.rect_fields(object) else {
            return Ok((o.position.into(), o.rotation(), o.scale.into()));
        };
        if let Some(c) = self.root_canvas(object)
            && c.render_mode != RenderMode::WorldSpace
        {
            return self.canvas_trs(object, r, c);
        }
        let mut p = Vec3::from(o.position);
        if let Some(parent) = self.metadata[&object].parent
            && let Some((_, pr)) = self.rect_fields(parent)
        {
            let size = self.rect_size(parent)?;
            let anchor = Vec2::from(r.anchor_min)
                + (Vec2::from(r.anchor_max) - Vec2::from(r.anchor_min)) * Vec2::from(r.pivot);
            let xy = size * (anchor - Vec2::from(pr.pivot)) + Vec2::from(r.anchored_position);
            p.x = xy.x;
            p.y = xy.y;
        } else {
            p.x = r.anchored_position[0];
            p.y = r.anchored_position[1];
        }
        Ok((p, o.rotation(), o.scale.into()))
    }
    fn canvas_trs(&self, object: u64, r: &RectTransform, c: &Canvas) -> Result<Trs, String> {
        if self.metadata[&object].parent.is_some() {
            return Err("screen-space Canvas must be a root object".into());
        }
        let factor = self.canvas_factor(object);
        let size = self.rect_size(object)?;
        let pivot = Vec2::from(r.pivot);
        if c.render_mode == RenderMode::ScreenSpaceOverlay {
            return Ok((
                (pivot * size * factor).extend(0.0),
                Quat::IDENTITY,
                Vec3::splat(factor),
            ));
        }
        let (camera, pose) = self.canvas_camera(c)?;
        let unit = 2.0 * camera.orthographic_size / self.ui.viewport[1] as f32;
        let xy = (pivot - Vec2::splat(0.5)) * size * factor * unit;
        Ok((
            pose.0 + pose.1 * Vec3::new(xy.x, xy.y, c.plane_distance),
            pose.1,
            Vec3::splat(factor * unit),
        ))
    }
    fn canvas_camera(&self, c: &Canvas) -> Result<(&Camera, Trs), String> {
        let id = c
            .camera
            .as_ref()
            .ok_or("camera-space Canvas requires camera reference")?
            .handle;
        let component = self.components[self.component_index(id)?].as_ref().unwrap();
        let Fields::Camera(camera) = &component.fields else {
            return Err("canvas camera reference requires Camera component".into());
        };
        if !camera.orthographic {
            return Err("native Canvas currently requires orthographic camera".into());
        }
        if self.canvas(component.object).is_some() {
            return Err("Canvas camera cannot be inside a Canvas hierarchy".into());
        }
        Ok((camera, self.world_trs(component.object)?))
    }
    pub(crate) fn rect_snapshot(&self, object: u64) -> Result<Value, String> {
        self.object_index(object)?;
        let (c, r) = self
            .rect_fields(object)
            .ok_or("object requires RectTransform")?;
        let size = self.rect_size(object)?;
        let min = -Vec2::from(r.pivot) * size;
        let max = min + size;
        let rect = [min.x, min.y, max.x, max.y];
        let corners = self.rect_corners(object, rect)?;
        Ok(
            json!({"object":object,"component":c.handle,"size":size.to_array(),"local_rect":rect,"world_corners":corners.world,"screen_corners":corners.screen}),
        )
    }
    pub(super) fn rect_corners(&self, object: u64, rect: [f32; 4]) -> Result<Corners, String> {
        let (p, r, s) = self.world_trs(object)?;
        let world: Vec<_> = [
            [rect[0], rect[1]],
            [rect[2], rect[1]],
            [rect[2], rect[3]],
            [rect[0], rect[3]],
        ]
        .map(|v| (p + r * (s * Vec3::new(v[0], v[1], 0.0))).to_array())
        .to_vec();
        let screen = if let Some((_, c)) = self.canvas(object) {
            if c.render_mode == RenderMode::ScreenSpaceOverlay {
                Some(world.iter().map(|p| [p[0], p[1]]).collect())
            } else if c.camera.is_some() {
                Some(self.project_corners(c, &world)?)
            } else {
                None
            }
        } else {
            None
        };
        Ok(Corners { world, screen })
    }
    fn project_corners(&self, c: &Canvas, world: &[[f32; 3]]) -> Result<Vec<[f32; 2]>, String> {
        let (camera, (p, r, _)) = self.canvas_camera(c)?;
        let height = self.ui.viewport[1] as f32;
        let width = self.ui.viewport[0] as f32;
        let unit = height / (2.0 * camera.orthographic_size);
        Ok(world
            .iter()
            .map(|v| {
                let local = r.inverse() * (Vec3::from(*v) - p);
                [width * 0.5 + local.x * unit, height * 0.5 + local.y * unit]
            })
            .collect())
    }
}

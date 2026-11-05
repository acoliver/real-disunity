//! Caller-owned layout rules over the world's existing rectangle hierarchy.
use super::{RectTransform, UiError};
use crate::{ObjectHandle, World, WorldError};
use glam::{UVec2, Vec2};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAlign {
    Start,
    Center,
    End,
    Stretch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiGridFlow {
    Columns(u16),
    Rows(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSize {
    Fixed(UVec2),
    Content,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLayoutGroup {
    Stack {
        axis: UiAxis,
        spacing: u16,
        padding: [u16; 4],
        align: UiAlign,
    },
    Grid {
        flow: UiGridFlow,
        cell: UVec2,
        spacing: UVec2,
        padding: [u16; 4],
    },
}
/// Rules are supplied at resolution time and never rewrite authored components.
#[derive(Clone, Debug, Default)]
pub struct UiLayoutRules {
    pub groups: BTreeMap<ObjectHandle, UiLayoutGroup>,
    pub sizes: BTreeMap<ObjectHandle, UiSize>,
    pub scrolls: BTreeMap<ObjectHandle, super::UiScrollView>,
}
impl UiLayoutRules {
    pub(super) fn prepare(
        &self,
        world: &World,
        rects: &mut BTreeMap<ObjectHandle, RectTransform>,
    ) -> Result<(), WorldError> {
        for &object in self.groups.keys().chain(self.sizes.keys()) {
            if !rects.contains_key(&object) {
                return Err(UiError::MissingLayout.into());
            }
        }
        let sizes = self
            .sizes
            .keys()
            .map(|&o| self.intrinsic(world, o, rects).map(|size| (o, size)))
            .collect::<Result<Vec<_>, _>>()?;
        for (object, size) in sizes {
            let rect = rects.get_mut(&object).expect("validated rectangle");
            rect.anchor_max = rect.anchor_min;
            rect.size_delta = size;
        }
        for (&object, group) in &self.groups {
            group.measure(
                &Self::children(world, object, rects)?
                    .iter()
                    .map(|&o| self.intrinsic(world, o, rects))
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
        }
        for (&viewport, scroll) in &self.scrolls {
            if !rects.contains_key(&viewport)
                || !rects.contains_key(&scroll.content)
                || world.parent(scroll.content)? != Some(viewport)
            {
                return Err(UiError::MissingLayout.into());
            }
        }
        Ok(())
    }
    fn children(
        world: &World,
        object: ObjectHandle,
        rects: &BTreeMap<ObjectHandle, RectTransform>,
    ) -> Result<Vec<ObjectHandle>, WorldError> {
        let mut children = Vec::new();
        for child in world.children(object)? {
            if rects.contains_key(&child) && world.active_in_hierarchy(child)? {
                children.push(child);
            }
        }
        Ok(children)
    }
    fn intrinsic(
        &self,
        world: &World,
        object: ObjectHandle,
        rects: &BTreeMap<ObjectHandle, RectTransform>,
    ) -> Result<Vec2, WorldError> {
        match self.sizes.get(&object) {
            Some(UiSize::Fixed(size)) => Ok(size.as_vec2()),
            Some(UiSize::Content) => {
                let group = self.groups.get(&object).ok_or(UiError::MissingLayout)?;
                let sizes = Self::children(world, object, rects)?
                    .into_iter()
                    .map(|o| self.intrinsic(world, o, rects))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(group.measure(&sizes)?)
            }
            None => Ok(rects[&object].size_delta.max(Vec2::ZERO).round()),
        }
    }
    pub(super) fn arrange(
        &self,
        world: &World,
        object: ObjectHandle,
        size: Vec2,
        rects: &mut BTreeMap<ObjectHandle, RectTransform>,
    ) -> Result<(), WorldError> {
        if let Some(group) = self.groups.get(&object) {
            let children = Self::children(world, object, rects)?;
            let sizes = children
                .iter()
                .map(|&o| self.intrinsic(world, o, rects))
                .collect::<Result<Vec<_>, _>>()?;
            for (child, at) in children.into_iter().zip(group.positions(size, &sizes)?) {
                let (at, extent) = at;
                place(rects.get_mut(&child).expect("child rectangle"), at, extent);
            }
        }
        if let Some(scroll) = self.scrolls.get(&object) {
            let content = rects
                .get_mut(&scroll.content)
                .expect("validated scroll content");
            let extent = content.size_delta.max(Vec2::ZERO).round();
            let offset = scroll
                .offset()
                .clamp(Vec2::ZERO, (extent - size).max(Vec2::ZERO));
            place(content, Vec2::new(-offset.x, -offset.y), extent);
        }
        Ok(())
    }
}
fn place(rect: &mut RectTransform, top_left: Vec2, size: Vec2) {
    rect.anchor_min = Vec2::new(0., 1.);
    rect.anchor_max = rect.anchor_min;
    rect.size_delta = size;
    rect.anchored_position =
        Vec2::new(top_left.x, -top_left.y) + Vec2::new(rect.pivot.x, rect.pivot.y - 1.) * size;
}
impl UiLayoutGroup {
    fn padding(self) -> [f32; 4] {
        match self {
            Self::Stack { padding, .. } | Self::Grid { padding, .. } => padding.map(f32::from),
        }
    }
    fn measure(self, sizes: &[Vec2]) -> Result<Vec2, UiError> {
        let count = u16::try_from(sizes.len()).map_err(|_| UiError::InvalidRect)?;
        let [left, right, top, bottom] = self.padding();
        let inner = match self {
            Self::Stack { axis, spacing, .. } => {
                let sum: Vec2 = sizes.iter().copied().sum();
                let max = sizes.iter().copied().fold(Vec2::ZERO, Vec2::max);
                let gaps = f32::from(count.saturating_sub(1)) * f32::from(spacing);
                match axis {
                    UiAxis::Horizontal => Vec2::new(sum.x + gaps, max.y),
                    UiAxis::Vertical => Vec2::new(max.x, sum.y + gaps),
                }
            }
            Self::Grid {
                flow,
                cell,
                spacing,
                ..
            } => {
                let dimensions = grid_dimensions(flow, count)?;
                dimensions * cell.as_vec2()
                    + (dimensions - Vec2::ONE).max(Vec2::ZERO) * spacing.as_vec2()
            }
        };
        Ok(inner + Vec2::new(left + right, top + bottom))
    }
    fn positions(self, size: Vec2, sizes: &[Vec2]) -> Result<Vec<(Vec2, Vec2)>, UiError> {
        let [left, right, top, bottom] = self.padding();
        let mut cursor = Vec2::new(left, top);
        let available = (size - Vec2::new(left + right, top + bottom)).max(Vec2::ZERO);
        let mut output = Vec::new();
        for (index, &extent) in sizes.iter().enumerate() {
            match self {
                Self::Stack {
                    axis,
                    spacing,
                    align,
                    ..
                } => {
                    let cross = match axis {
                        UiAxis::Horizontal => 1,
                        UiAxis::Vertical => 0,
                    };
                    let main = 1 - cross;
                    let mut extent = extent;
                    if align == UiAlign::Stretch {
                        extent[cross] = available[cross].round();
                    }
                    let mut at = cursor;
                    at[cross] += alignment(align, available[cross] - extent[cross]);
                    output.push((at.round(), extent));
                    cursor[main] += extent[main] + f32::from(spacing);
                }
                Self::Grid {
                    flow,
                    cell,
                    spacing,
                    ..
                } => {
                    let index = u16::try_from(index).map_err(|_| UiError::InvalidRect)?;
                    let coordinate = grid_coordinate(flow, index)?;
                    output.push((
                        cursor + coordinate * (cell.as_vec2() + spacing.as_vec2()),
                        cell.as_vec2(),
                    ));
                }
            }
        }
        Ok(output)
    }
}
fn alignment(align: UiAlign, spare: f32) -> f32 {
    match align {
        UiAlign::Start | UiAlign::Stretch => 0.,
        UiAlign::Center => (spare * 0.5).floor(),
        UiAlign::End => spare,
    }
}
fn grid_dimensions(flow: UiGridFlow, count: u16) -> Result<Vec2, UiError> {
    match flow {
        UiGridFlow::Columns(0) | UiGridFlow::Rows(0) => Err(UiError::InvalidRect),
        UiGridFlow::Columns(n) => Ok(Vec2::new(
            f32::from(n.min(count)),
            f32::from(count.div_ceil(n)),
        )),
        UiGridFlow::Rows(n) => Ok(Vec2::new(
            f32::from(count.div_ceil(n)),
            f32::from(n.min(count)),
        )),
    }
}
fn grid_coordinate(flow: UiGridFlow, index: u16) -> Result<Vec2, UiError> {
    match flow {
        UiGridFlow::Columns(0) | UiGridFlow::Rows(0) => Err(UiError::InvalidRect),
        UiGridFlow::Columns(n) => Ok(Vec2::new(f32::from(index % n), f32::from(index / n))),
        UiGridFlow::Rows(n) => Ok(Vec2::new(f32::from(index / n), f32::from(index % n))),
    }
}

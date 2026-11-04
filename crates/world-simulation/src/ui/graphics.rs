//! The quads of the meshes canvas renderers submit, ready for the presentation.
//! Corners are in the same space as a text glyph's: screen pixels on a screen-space canvas,
//! native world points otherwise.
use crate::presentation::SpriteLighting;
use crate::presentation::extraction::{
    SpriteQuad, WorldRenderPlan, camera_admits, selected_camera, ui_corners,
};
use crate::ui::clip::{CanvasClip, Clipped};
use crate::ui::group::{effective_alpha, groups_by_object};
use crate::ui::{CanvasRenderer, RectLayout, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiGraphicQuad {
    pub corners: [[f32; 3]; 4],
    /// As `SpriteQuad::uv_rect`.
    pub uv_rect: [f32; 4],
    /// The renderer's colour times the quad's vertex colour times the inherited alpha.
    pub color: [f32; 4],
}

/// What one canvas renderer draws.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGraphic {
    pub object: ObjectHandle,
    pub canvas: ObjectHandle,
    pub overlay: bool,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    /// The renderer identity of the texture the quads sample, or none for plain colour.
    pub texture: Option<u64>,
    pub quads: Vec<UiGraphicQuad>,
    /// Triangles of the mesh that make no quad and are not drawn.
    pub left_out_triangles: usize,
}

/// The graphics of every canvas renderer that is drawn, in hierarchy order: depth first, siblings
/// in the order the world lists them, roots in object order. A renderer is drawn when its object
/// has a rectangle in `layout` (so it is active under an enabled canvas), it is not culled, it
/// has a mesh, and the alpha it inherits is above zero.
pub fn extract_graphics(world: &World, layout: &UiLayout) -> Result<Vec<UiGraphic>, WorldError> {
    let groups = groups_by_object(world.published_components());
    let camera = selected_camera(world, world.published_components())?.map(|(_, camera)| camera);
    let rects: BTreeMap<ObjectHandle, &RectLayout> =
        layout.rects.iter().map(|r| (r.object, r)).collect();
    let order = hierarchy_order(world)?;
    let roots = root_canvases(world)?;
    let mut found = Vec::new();
    for component in world.published_components() {
        let ComponentDesc::CanvasRenderer(renderer) = &component.desc else {
            continue;
        };
        let Some(rect) = rects.get(&component.object) else {
            continue;
        };
        let alpha = renderer.alpha * effective_alpha(world, &groups, component.object)?;
        let overlay = rect.on_screen();
        if renderer.flags.cull
            || alpha <= 0.
            || (!overlay && !camera_admits(world, camera, component.object)?)
        {
            continue;
        }
        let clip = renderer.clip_rect.and_then(|clip| {
            let root = rects.get(roots.get(&rect.canvas)?)?;
            CanvasClip::new(clip, root.local_to_world)
        });
        if let Some(graphic) = graphic(renderer, rect, overlay, alpha, clip) {
            let at = order.get(&component.object).copied().unwrap_or(usize::MAX);
            found.push((at, graphic));
        }
    }
    found.sort_by_key(|(at, _)| *at);
    Ok(found.into_iter().map(|(_, graphic)| graphic).collect())
}

fn graphic(
    renderer: &CanvasRenderer,
    rect: &RectLayout,
    overlay: bool,
    alpha: f32,
    clip: Option<CanvasClip>,
) -> Option<UiGraphic> {
    let found = renderer.geometry.as_ref()?.quads();
    let tint = [
        renderer.color[0],
        renderer.color[1],
        renderer.color[2],
        renderer.color[3] * alpha,
    ];
    let quads = found
        .quads
        .iter()
        .filter_map(|quad| {
            let world = quad
                .positions
                .map(|p| rect.local_to_world.transform_point3(glam::Vec3::from(p)));
            let (world, uv_rect) =
                match clip.map_or(Clipped::Whole, |c| c.clip(world, quad.uv_rect)) {
                    Clipped::Whole => (world, quad.uv_rect),
                    Clipped::Outside => return None,
                    Clipped::Cut(world, uv_rect) => (world, uv_rect),
                };
            Some(UiGraphicQuad {
                corners: world.map(|p| rect.drawn_world_point(p).to_array()),
                uv_rect,
                color: std::array::from_fn(|i| tint[i] * quad.color[i]),
            })
        })
        .collect();
    Some(UiGraphic {
        object: rect.object,
        canvas: rect.canvas,
        overlay,
        sorting_layer: rect.sorting_layer,
        sorting_order: rect.sorting_order,
        texture: (renderer.graphic_texture != 0).then_some(renderer.graphic_texture),
        quads,
        left_out_triangles: found.left_out_triangles,
    })
}

/// The topmost canvas above (or at) each canvas object: the canvas whose space clip rectangles
/// are measured in.
pub(super) fn root_canvases(
    world: &World,
) -> Result<BTreeMap<ObjectHandle, ObjectHandle>, WorldError> {
    let canvases: Vec<ObjectHandle> = world
        .published_components()
        .filter(|c| matches!(c.desc, ComponentDesc::Canvas(_)))
        .map(|c| c.object)
        .collect();
    let mut roots = BTreeMap::new();
    for &canvas in &canvases {
        let mut root = canvas;
        let mut current = world.parent(canvas)?;
        while let Some(node) = current {
            if canvases.contains(&node) {
                root = node;
            }
            current = world.parent(node)?;
        }
        roots.insert(canvas, root);
    }
    Ok(roots)
}

/// Each object's place in a depth-first walk of the hierarchy.
fn hierarchy_order(world: &World) -> Result<BTreeMap<ObjectHandle, usize>, WorldError> {
    let mut order = BTreeMap::new();
    for object in world.published_objects() {
        if world.parent(object)?.is_some() {
            continue;
        }
        for node in world.subtree(object)? {
            order.insert(node, order.len());
        }
    }
    Ok(order)
}

/// Adds the quads of `graphics` to the frame, in the order given. The frame sorts its quads
/// stably by overlay, layer and order, so quads with equal keys keep the hierarchy order
/// `extract_graphics` returns. Call it before `append_ui_from` to draw the quads under the
/// engine's own text of the same sorting key.
pub fn append_graphics(
    plan: &mut WorldRenderPlan,
    graphics: Vec<UiGraphic>,
) -> Result<(), worldrunner_camera::ViewError> {
    for graphic in graphics {
        for quad in graphic.quads {
            plan.quads.push(SpriteQuad {
                lighting: SpriteLighting::Unlit,
                overlay: graphic.overlay,
                corners: ui_corners(plan.camera.as_ref(), quad.corners, graphic.overlay)?,
                uv_rect: quad.uv_rect,
                color: quad.color,
                texture: graphic.texture,
                sorting_layer: graphic.sorting_layer,
                sorting_order: graphic.sorting_order,
            });
        }
    }
    plan.quads
        .sort_by_key(|q| (q.overlay, q.sorting_layer, q.sorting_order));
    Ok(())
}

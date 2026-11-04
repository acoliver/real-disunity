//! Pathless CPU UI contributions for the existing scene compositor.
//! Texture coverage is linear R8; a glyph quad samples it as alpha. Overlay
//! vertices are pixels, other vertices use the native world basis. Rendering
//! and GPU resource lifetimes remain the scene renderer's responsibility.

pub use worldrunner_world_simulation::ui::contribution::{
    UiGlyphQuad, UiRenderPlan, UiTextContribution, extract_ui,
};

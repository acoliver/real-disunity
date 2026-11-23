//! The material a canvas renderer draws with, as the UI presentation names it.
use crate::World;
use crate::graphics::{MaterialId, WorldShader};

/// The shader the engine supplies for the UI's own look.
pub const ENGINE_UI_SHADER: &str = "UI/Default";

/// A material whose shader the game supplies, as a UI draw names it. The generation is the
/// material's own, so a change to its properties shows in a plan comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiMaterial {
    pub id: MaterialId,
    pub generation: u64,
}

/// Whether the engine draws a UI graphic with `shader` itself: the shader has no source or is the
/// engine's own UI shader. Every other shader is a game's program.
#[must_use]
pub fn draws_natively(shader: &WorldShader) -> bool {
    shader.source.is_none() || shader.name == ENGINE_UI_SHADER
}

impl UiMaterial {
    /// The material `id` names, when it is live and the game supplies its shader. A missing,
    /// destroyed or natively drawn material gives none.
    pub(crate) fn of(world: &World, id: Option<MaterialId>) -> Option<Self> {
        let id = id?;
        let material = world.material(id).ok()?;
        let shader = world.shader(material.shader).ok()?;
        (!draws_natively(shader)).then_some(Self {
            id,
            generation: material.generation,
        })
    }

    /// The report line for a draw left out of the layers, with the reason it cannot be drawn.
    #[must_use]
    pub fn refusal(self, shader: &str, reason: &str) -> String {
        format!(
            "UI material {} (shader {shader}) is left out: {reason}",
            self.id.to_bits()
        )
    }
}

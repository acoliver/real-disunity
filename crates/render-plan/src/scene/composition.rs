//! Activation-bound scene factories bridging authored routes into
//! renderer-domain contributions and one composed scene.
//!
//! Factories, never generic role interpretation, produce renderer semantics:
//! each route scene factory receives the complete exact authored declaration
//! plus its prepared [`RenderPayload`] and returns one closed
//! [`SceneContribution`] tagged by renderer meaning, and the document scene
//! composition factory receives the exact retained
//! [`ContractDocument`](crate::document::ContractDocument), the root payload,
//! and every produced contribution with exact claimant provenance, and
//! returns one validated
//! [`RenderScene`](crate::scene::RenderScene).

use crate::adapter::RenderPayload;
use crate::document::{
    CapabilityRouteDeclaration, ComponentRouteDeclaration, DocumentRouteDeclaration,
};
use crate::implementation_registry::RenderFactoryError;
use crate::scene::geometry::{SceneMesh, SceneTransform};
use crate::scene::resource::{SceneMaterial, SceneTexture};
use crate::scene::terrain::SceneTerrain;

/// One rigid instantiation of a mesh, naming that mesh by logical identity.
///
/// A route scene factory cannot address its mesh by a dense [`crate::scene::MeshHandle`],
/// because handle allocation is the document composer's exclusive
/// responsibility and no single route observes the final table order. Nor can
/// every placement-producing route resolve the mesh itself: the route that
/// carries object placements need not carry the asset-index role that maps a
/// logical identity onto published mesh bytes. The placement therefore states
/// only what it authoritatively knows -- an exact transform and the exact
/// logical identity it instantiates -- and the document composer, which alone
/// observes every contribution, resolves that identity against the mesh and
/// material tables it composes.
///
/// Resolution failure is a composition rejection, never a silent default: an
/// unresolvable identity must not collapse to handle zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenePlacement {
    transform: SceneTransform,
    mesh_identity: String,
}

impl ScenePlacement {
    /// Adopts one transform with the exact logical identity it instantiates.
    #[must_use]
    pub const fn new(transform: SceneTransform, mesh_identity: String) -> Self {
        Self {
            transform,
            mesh_identity,
        }
    }

    /// Returns the exact rigid transform of this instantiation.
    #[must_use]
    pub const fn transform(&self) -> &SceneTransform {
        &self.transform
    }

    /// Returns the exact logical identity the document composer must resolve.
    #[must_use]
    pub fn mesh_identity(&self) -> &str {
        &self.mesh_identity
    }
}

/// One closed renderer-domain contribution produced by one route scene factory.
///
/// The variant, not the route or role, carries the renderer meaning: terrain
/// heights cannot masquerade as textures and billboards cannot masquerade as
/// meshes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneContribution {
    /// One owned texture.
    Texture(SceneTexture),
    /// One owned material.
    Material(SceneMaterial),
    /// One owned mesh with its LOD chain.
    Mesh(SceneMesh),
    /// One owned terrain grid whose material palette resolves against the
    /// composed scene's material table.
    Terrain(SceneTerrain),
    /// One owned height sample grid, in whole millimeters, for the terrain
    /// recipe the document composes. The document composer must validate the
    /// exact row-major sample count against that recipe before constructing a
    /// [`SceneTerrain`].
    HeightField {
        /// Row-major whole-millimeter heights.
        heights_millimeters: Box<[i32]>,
    },
    /// Owned rigid placements that instantiate meshes by logical identity. The
    /// document composer must resolve every [`ScenePlacement::mesh_identity`]
    /// against the mesh and material tables it composes before constructing a
    /// [`SceneObject`], rejecting any identity it cannot resolve.
    ///
    /// [`SceneObject`]: crate::SceneObject
    Placements(Box<[ScenePlacement]>),
}

/// One produced contribution with its exact claimant provenance.
///
/// The claimant is the exact component or binding identifier whose route
/// factory produced the contribution, so the document composer can attribute
/// every part of the scene without interpreting roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenSceneContribution {
    claimant: String,
    contribution: SceneContribution,
}

impl ProvenSceneContribution {
    /// Adopts one contribution with its exact claimant.
    #[must_use]
    pub fn new(claimant: String, contribution: SceneContribution) -> Self {
        Self {
            claimant,
            contribution,
        }
    }

    /// Returns the exact producing claimant identifier.
    #[must_use]
    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    /// Returns the renderer-domain contribution.
    #[must_use]
    pub const fn contribution(&self) -> &SceneContribution {
        &self.contribution
    }
}

/// The complete exact authored declaration one route scene factory serves.
///
/// The closed variant preserves route-kind separation: a document scene
/// factory never receives component or capability declarations and vice
/// versa, and the whole declaration is passed verbatim rather than a
/// claimant string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteDeclarationRef<'a> {
    /// The single document route declaration.
    Document(&'a DocumentRouteDeclaration),
    /// One component route declaration.
    Component(&'a ComponentRouteDeclaration),
    /// One capability route declaration.
    Capability(&'a CapabilityRouteDeclaration),
}

/// Everything one route scene factory receives.
#[derive(Clone, Copy, Debug)]
pub struct RouteSceneInput<'a> {
    declaration: RouteDeclarationRef<'a>,
    payload: &'a RenderPayload,
}

impl<'a> RouteSceneInput<'a> {
    /// Bundles one exact declaration with its prepared payload.
    #[must_use]
    pub const fn new(declaration: RouteDeclarationRef<'a>, payload: &'a RenderPayload) -> Self {
        Self {
            declaration,
            payload,
        }
    }

    /// Returns the complete exact authored declaration.
    #[must_use]
    pub const fn declaration(&self) -> &RouteDeclarationRef<'a> {
        &self.declaration
    }

    /// Returns the prepared owned CPU payload of this route's contribution.
    #[must_use]
    pub const fn payload(&self) -> &'a RenderPayload {
        self.payload
    }
}

/// Everything the document scene composition factory receives.
#[derive(Clone, Copy, Debug)]
pub struct DocumentSceneInput<'a> {
    document: &'a crate::document::ContractDocument,
    root_payload: &'a RenderPayload,
    contributions: &'a [ProvenSceneContribution],
}

impl<'a> DocumentSceneInput<'a> {
    /// Bundles the retained document, root payload, and proven contributions.
    #[must_use]
    pub const fn new(
        document: &'a crate::document::ContractDocument,
        root_payload: &'a RenderPayload,
        contributions: &'a [ProvenSceneContribution],
    ) -> Self {
        Self {
            document,
            root_payload,
            contributions,
        }
    }

    /// Returns the exact retained authored contract document.
    #[must_use]
    pub const fn document(&self) -> &'a crate::document::ContractDocument {
        self.document
    }

    /// Returns the adapted root payload of the document route.
    #[must_use]
    pub const fn root_payload(&self) -> &'a RenderPayload {
        self.root_payload
    }

    /// Returns every produced contribution with exact claimant provenance.
    #[must_use]
    pub fn contributions(&self) -> &[ProvenSceneContribution] {
        self.contributions
    }
}

/// Object-safe route scene contribution factory.
///
/// One instance serves exactly one authored route declaration; it receives
/// the complete declaration and its prepared payload and returns the closed
/// renderer meaning of that route's content. The same payload contract on a
/// different route may carry a distinct factory.
pub trait RouteSceneFactory: Send + Sync {
    /// Produces one renderer-domain contribution for one exact declaration.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFactoryError`] when the payload does not satisfy this
    /// factory's exact scene semantics.
    fn compose_contribution(
        &self,
        input: RouteSceneInput<'_>,
    ) -> Result<SceneContribution, RenderFactoryError>;
}

/// Object-safe document scene composition factory.
///
/// The factory composes the complete [`RenderScene`](crate::scene::RenderScene)
/// from the exact retained contract document, the root payload, and every
/// produced route contribution; it is bound at activation and invoked
/// exactly once per plan build.
pub trait SceneCompositionFactory: Send + Sync {
    /// Composes one validated scene from the exact retained document.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFactoryError`] when the contributions cannot compose
    /// the document's scene.
    fn compose_scene(
        &self,
        input: DocumentSceneInput<'_>,
    ) -> Result<crate::scene::RenderScene, RenderFactoryError>;
}

//! The stock frame plan, built in one place and cached by what decides it.
//!
//! A frame's passes come from several contributors: the scene or the shadowed scene authors the
//! base passes (the opaque pass, the shadow casters, the celestial pass, velocity), and the
//! screen-space effects, reflections, retained features and snapshots add their own around them.
//! This module collects them in that order, applies the one forward order and positions the
//! operations group, for the unfitted frame, the frame that follows a depth prefix and the prefix
//! itself.
//!
//! The result splits in two. The **logical schedule** is the entries, their roles and
//! occurrences, the attachment resources they name, the declared effects and the position of
//! the operations group. It is a function of the settings the renderer was built from, the
//! extents and formats of what it allocated and what the caller authored, so it is cached under
//! those ([`shape`]) and rebuilt only when they change. The **binding table** is the checked
//! views those attachments select this frame. It is a function of the frame slot, the
//! allocations behind the renderer and the caller views' retained-content permissions. Each
//! schedule keeps the tables it has met and selects one by those inputs. A moving camera, a
//! rotating frame slot or a renderer replaced by an identical configuration therefore reuses
//! the schedule and only changes the binding table.
//!
//! Frames that have features the host registered are planned every time: a registered feature's
//! plan is host code that may depend on anything, so nothing decides it but running it.
mod base;
mod cache;
mod frame;
mod prefix;
mod shadowed;
mod shape;

pub(crate) use base::{Base, SceneInputs, unshadowed};
pub(crate) use cache::PlanCache;
pub use cache::StockPlanStatistics;
pub(super) use frame::{Authored, Environment, Parts, plan_frame};
pub(crate) use shadowed::ShadowSchedule;

use crate::water::planning::SnapshotRequest;
use worldrunner_render_post::{FeatureDeclaration, FeaturePass, PassDeclaration, PassEntry};

/// The schedule of a frame: what every frame with the same key submits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LogicalSchedule {
    /// Every pass of the frame in submission order.
    pub passes: Vec<PassDeclaration>,
    /// The caller's feature declarations, with the colour layout of the pass each draws in.
    pub features: Vec<FeatureDeclaration>,
    pub screen: Option<FeatureDeclaration>,
    pub reflection: Option<FeatureDeclaration>,
    pub snapshot: Option<FeatureDeclaration>,
    /// What the retained features declared when they planned.
    pub retained: Vec<FeatureDeclaration>,
    pub request: SnapshotRequest,
    /// The entry an operations group runs immediately before.
    pub first: Option<PassEntry>,
}

/// The schedule of a depth prefix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LogicalPrefix {
    pub passes: Vec<PassDeclaration>,
    /// What the scene feature declares for the prefix's prepass.
    pub prepass: FeaturePass,
    pub screen: FeatureDeclaration,
}

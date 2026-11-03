use super::*;
use crate::retained::resource_edits::tests::{
    append, combined, material_on, mesh, object, texture,
};
use crate::{ObjectId, RetainedHost, RetainedUpdate};
use worldrunner_render_scene::draw::ResourceBase;

struct Fixture {
    context: GpuContext,
    host: RetainedHost,
    resident: Resident,
    streaming: super::super::readiness::Streaming,
    target: OffscreenTarget,
}

impl Fixture {
    fn new(shadows: Option<DirectionalShadowSettings>) -> Self {
        let context =
            pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
        let resident =
            Resident::new(&context, &source(), shadows, crate::FrameProfile::Legacy).unwrap();
        let target = OffscreenTarget::new(
            context.device(),
            Viewport {
                width: 64,
                height: 64,
            },
        )
        .unwrap();
        Self {
            host: RetainedHost::new(source(), crate::tests::options()).unwrap(),
            resident,
            streaming: super::super::readiness::Streaming::new(
                shadows,
                crate::FrameProfile::Legacy,
            ),
            target,
            context,
        }
    }

    fn apply(&mut self, update: RetainedUpdate) {
        self.host.apply(update).unwrap();
        publish(
            &self.context,
            &mut self.streaming,
            &mut self.resident,
            &mut self.host,
        );
    }

    fn frame(&mut self) -> RgbaImage {
        frame(&mut self.resident, &self.context, &mut self.target)
    }

    /// The frame of a resident built from scratch for the scene the host now holds.
    fn rebuilt_frame(&mut self, shadows: Option<DirectionalShadowSettings>) -> RgbaImage {
        let objects = self.host.objects().map(|(_, object)| object).collect();
        let scene = scene_with_objects(self.host.scene(), objects);
        let mut fresh =
            Resident::new(&self.context, &scene, shadows, crate::FrameProfile::Legacy).unwrap();
        frame(&mut fresh, &self.context, &mut self.target)
    }
}

fn both_residents() -> [Option<DirectionalShadowSettings>; 2] {
    [None, Some(shadow_settings())]
}

#[test]
fn a_frame_after_an_append_matches_a_frame_after_the_full_replacement() {
    for shadows in both_residents() {
        let mut fixture = Fixture::new(shadows);
        let before = fixture.frame();
        fixture.apply(RetainedUpdate::AppendResources(append()));
        fixture.apply(RetainedUpdate::Add {
            id: ObjectId(1),
            object: object(1, 1),
        });
        let appended = fixture.frame();
        assert_ne!(appended, before);
        assert_eq!(appended, fixture.rebuilt_frame(shadows));
        assert_eq!(
            fixture.resident.scene_mut().resource_base(),
            ResourceBase {
                textures: 2,
                materials: 2,
                meshes: 3
            }
        );
    }
}

#[test]
fn an_append_that_changes_no_object_leaves_the_frame_alone() {
    let mut fixture = Fixture::new(None);
    let before = fixture.frame();
    fixture.apply(RetainedUpdate::AppendResources(append()));
    assert_eq!(fixture.frame(), before);
}

#[test]
fn later_appends_may_use_earlier_items_and_edit_objects_in_the_same_frame() {
    for shadows in both_residents() {
        let mut fixture = Fixture::new(shadows);
        fixture.apply(RetainedUpdate::AppendResources(append()));
        fixture
            .host
            .apply(RetainedUpdate::AppendResources(
                worldrunner_render_plan::SceneAppend {
                    textures: vec![texture(90)],
                    materials: vec![material_on(2), material_on(1)],
                    meshes: vec![mesh(120)],
                },
            ))
            .unwrap();
        fixture
            .host
            .apply(RetainedUpdate::Objects(vec![
                crate::SceneObjectEdit::Add {
                    id: ObjectId(1),
                    object: object(2, 1),
                },
                crate::SceneObjectEdit::Add {
                    id: ObjectId(2),
                    object: object(3, 3),
                },
            ]))
            .unwrap();
        publish(
            &fixture.context,
            &mut fixture.streaming,
            &mut fixture.resident,
            &mut fixture.host,
        );
        assert_eq!(fixture.frame(), fixture.rebuilt_frame(shadows));
    }
}

#[test]
fn replacing_a_mesh_changes_the_next_frame_to_the_rebuilt_one() {
    for shadows in both_residents() {
        let mut fixture = Fixture::new(shadows);
        let before = fixture.frame();
        let handles: Vec<_> = fixture.resident.scene_mut().instance_handles().collect();
        fixture.apply(RetainedUpdate::ReplaceMesh {
            handle: worldrunner_render_plan::MeshHandle::from_raw(0),
            mesh: mesh(300),
        });
        let replaced = fixture.frame();
        assert_ne!(replaced, before);
        assert_eq!(replaced, fixture.rebuilt_frame(shadows));
        assert_eq!(
            fixture
                .resident
                .scene_mut()
                .instance_handles()
                .collect::<Vec<_>>(),
            handles
        );
    }
}

#[test]
fn replacing_a_texture_changes_the_next_frame_to_the_rebuilt_one() {
    for shadows in both_residents() {
        let mut fixture = Fixture::new(shadows);
        let before = fixture.frame();
        fixture.apply(RetainedUpdate::ReplaceTexture {
            handle: worldrunner_render_plan::TextureHandle::from_raw(0),
            texture: texture(120),
        });
        let replaced = fixture.frame();
        assert_ne!(replaced, before);
        assert_eq!(replaced, fixture.rebuilt_frame(shadows));
    }
}

#[test]
fn an_append_counts_only_its_own_bytes_as_uploaded() {
    let mut fixture = Fixture::new(None);
    let initial = fixture.resident.scene_mut().take_statistics();
    assert_eq!(
        initial.textures.uploaded_bytes,
        initial.textures.resident_bytes
    );
    fixture.apply(RetainedUpdate::AppendResources(append()));
    let appended = fixture.resident.scene_mut().take_statistics();
    let full = Resident::new(
        &fixture.context,
        &combined(&source(), &append()),
        None,
        crate::FrameProfile::Legacy,
    )
    .unwrap()
    .scene_mut()
    .take_statistics();
    assert_eq!(
        appended.textures.resident_bytes,
        full.textures.resident_bytes
    );
    assert_eq!(
        appended.mesh_buffers.resident_bytes,
        full.mesh_buffers.resident_bytes
    );
    assert_eq!(
        appended.textures.uploaded_bytes,
        full.textures.resident_bytes - initial.textures.resident_bytes
    );
    assert_eq!(
        appended.mesh_buffers.uploaded_bytes,
        full.mesh_buffers.resident_bytes - initial.mesh_buffers.resident_bytes
    );
    assert_eq!(
        fixture
            .resident
            .scene_mut()
            .take_statistics()
            .textures
            .uploaded_bytes,
        0
    );
}

fn large_texture() -> worldrunner_render_plan::SceneTexture {
    worldrunner_render_plan::SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1024,
        height: 1024,
        mips: 1,
        bytes: vec![7; 1024 * 1024 * 4].into(),
    }
}

fn budgeted_presentation(context: &GpuContext, headroom: u64) -> crate::presentation::Presentation {
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let policy = |budget: crate::PresentationBudget| crate::presentation::PresentationPolicy {
        budget: budget.into(),
        capture_pool: crate::CapturePoolPolicy::default(),
        recipes: false,
    };
    let make = |budget| {
        crate::presentation::Presentation::with_settings(
            context,
            COLOR,
            [size, size],
            crate::FrameProfile::Legacy,
            None,
            &policy(budget),
        )
        .unwrap()
    };
    let probe = make(crate::PresentationBudget(u64::MAX / 2));
    let resident = probe
        .scheduler
        .resident_bytes(&context.device().limits())
        .unwrap();
    make(crate::PresentationBudget(resident + headroom))
}

#[test]
fn an_append_over_the_budget_fails_and_changes_nothing() {
    let mut fixture = Fixture::new(None);
    let mut presentation = budgeted_presentation(&fixture.context, 1024 * 1024);
    let before = fixture.frame();
    let base = fixture.resident.scene_mut().resource_base();
    fixture
        .host
        .apply(RetainedUpdate::AppendResources(
            worldrunner_render_plan::SceneAppend {
                textures: vec![large_texture()],
                materials: vec![material_on(1)],
                meshes: Vec::new(),
            },
        ))
        .unwrap();
    let error = fixture
        .streaming
        .publish(
            &fixture.context,
            &mut fixture.resident,
            &mut presentation,
            &mut fixture.host,
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            crate::ViewerError::Configuration(
                worldrunner_render_gpu::configuration::ConfigurationError::Budget { .. }
            )
        ),
        "{error}"
    );
    assert_eq!(fixture.resident.scene_mut().resource_base(), base);
    assert_eq!(fixture.frame(), before);
}

#[test]
fn an_append_inside_the_budget_is_admitted() {
    let mut fixture = Fixture::new(None);
    let mut presentation = budgeted_presentation(&fixture.context, 64 * 1024 * 1024);
    fixture
        .host
        .apply(RetainedUpdate::AppendResources(
            worldrunner_render_plan::SceneAppend {
                textures: vec![large_texture()],
                materials: vec![material_on(1)],
                meshes: Vec::new(),
            },
        ))
        .unwrap();
    fixture
        .streaming
        .publish(
            &fixture.context,
            &mut fixture.resident,
            &mut presentation,
            &mut fixture.host,
        )
        .unwrap();
    assert_eq!(fixture.resident.scene_mut().resource_base().textures, 2);
}

fn windowless_host() -> (RetainedHost, Renderer) {
    let mut host = RetainedHost::new(source(), crate::tests::options()).unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&mut host)).unwrap();
    (host, renderer)
}

#[test]
fn the_edit_after_a_resource_edit_publishes_in_the_same_frame() {
    let (mut host, mut renderer) = windowless_host();
    host.apply(RetainedUpdate::AppendResources(append()))
        .unwrap();
    host.apply(RetainedUpdate::Add {
        id: ObjectId(1),
        object: object(1, 1),
    })
    .unwrap();
    host.apply(RetainedUpdate::Add {
        id: ObjectId(2),
        object: object(2, 1),
    })
    .unwrap();
    renderer.publish_retained(&mut host).unwrap();
    assert_eq!(
        host.ready_generation(),
        2,
        "the append and the object after it"
    );
    assert_eq!(host.accepted_generation(), 3);
    renderer.publish_retained(&mut host).unwrap();
    assert_eq!(host.ready_generation(), 3);
}

#[test]
fn object_edits_alone_still_publish_one_generation_per_frame() {
    let (mut host, mut renderer) = windowless_host();
    for id in 1..=2 {
        host.apply(RetainedUpdate::Add {
            id: ObjectId(id),
            object: object(0, 0),
        })
        .unwrap();
    }
    renderer.publish_retained(&mut host).unwrap();
    assert_eq!(host.ready_generation(), 1);
}

use super::*;

fn identity() -> crate::AdapterIdentity {
    crate::AdapterIdentity {
        name: "synthetic surface adapter".into(),
        class: crate::AdapterClass::Integrated,
        backend: wgpu::Backend::Metal,
    }
}

#[test]
fn desktop_supplier_receives_physical_extent_instead_of_logical_window_size() {
    let logical = winit::dpi::LogicalSize::new(640_u32, 480_u32);
    let physical = logical.to_physical::<u32>(2.0);
    let mut options = crate::tests::options();
    options.size = [logical.width, logical.height];
    options.window_size_unit = crate::WindowSizeUnit::Logical;
    let mut host = RetainedHost::with_initial_content(options, move |context| {
        assert_eq!(context.adapter, &identity());
        assert_eq!(context.surface_extent, [physical.width, physical.height]);
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap();
    host.resolve_initial_content(crate::InitialContext {
        adapter: &identity(),
        surface_extent: [physical.width, physical.height],
        display: None,
    })
    .unwrap();
    assert_eq!(host.size(), [1280, 960]);
    assert_eq!(host.view().unwrap().unwrap().viewport().height, 960);
    assert_eq!(host.accepted_generation(), 0);
}

#[test]
fn minimized_initial_extent_is_preserved_and_rendering_remains_suspended() {
    for surface_extent in [[0, 0], [0, 480], [640, 0]] {
        let mut host =
            RetainedHost::with_initial_content(crate::tests::options(), move |context| {
                assert_eq!(context.surface_extent, surface_extent);
                Ok(InitialContent {
                    scene: crate::tests::scene(false),
                    settings: None,
                })
            })
            .unwrap();
        host.resolve_initial_content(crate::InitialContext {
            adapter: &identity(),
            surface_extent,
            display: None,
        })
        .unwrap();
        assert_eq!(host.size(), surface_extent);
        assert!(host.view().unwrap().is_none());
        assert!(
            host.input(crate::RetainedInput::Resized(surface_extent))
                .is_empty()
        );
        assert_eq!(
            host.input(crate::RetainedInput::Resized([640, 480])),
            vec![
                crate::RetainedInput::DisplayChanged {
                    scale_factor: 1.,
                    physical_size: [640, 480]
                },
                crate::RetainedInput::Resized([640, 480])
            ]
        );
        assert!(host.view().unwrap().is_some());
    }
}

#[test]
fn supplier_sees_the_monitor_before_the_first_frame() {
    let display = crate::DisplayInfo {
        physical_size: [3840, 2160],
        scale_factor: 1.5,
    };
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let record = seen.clone();
    let mut options = crate::tests::options();
    options.window_mode = crate::WindowMode::BorderlessFullscreen;
    let mut host = RetainedHost::with_initial_content(options, move |context| {
        *record.lock().unwrap() = Some(context.display);
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap();
    host.resolve_initial_content(crate::InitialContext {
        adapter: &identity(),
        surface_extent: display.physical_size,
        display: Some(display),
    })
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(Some(display)));
    assert_eq!(host.size(), display.physical_size);
}

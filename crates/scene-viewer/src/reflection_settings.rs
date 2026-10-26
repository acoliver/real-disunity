use crate::{FrameProfile, ViewerError, ViewerOptions};
use worldrunner_render_plan::RenderScene;

pub(crate) fn startup_scene(
    scene: RenderScene,
    options: &ViewerOptions,
) -> Result<RenderScene, ViewerError> {
    let Some(settings) = options.ssr else {
        return Ok(scene);
    };
    settings
        .validate()
        .map_err(|error| ViewerError::Driver(error.to_string()))?;
    if matches!(options.frame_profile, FrameProfile::Legacy) {
        return Err(ViewerError::UnsupportedScene("SSR requires HDR"));
    }
    let effects = scene
        .effects()
        .ok_or(ViewerError::InvalidOptions(
            "SSR override requires authored scene effects",
        ))?
        .clone()
        .with_ssr(settings)
        .map_err(|error| ViewerError::Driver(error.to_string()))?;
    scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .map_err(|error| ViewerError::Driver(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::{SceneSsr, SceneSsrComposition};

    #[test]
    fn viewer_options_reach_retained_initial_settings() {
        let settings = SceneSsr {
            max_distance_millimeters: 42_000,
            thickness_slope_micro: 20_000,
            origin_skip_pixels: 4,
            pass_behind_steps: 8,
            roughness_fade_start_micro: Some(200_000),
            luminance_limit_milli: Some(2000),
            composition: SceneSsrComposition::ReplaceEnvironment,
            ..Default::default()
        };
        let options = ViewerOptions {
            ssr: Some(settings),
            frame_profile: FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        };
        let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
        assert_eq!(host.scene.effects().unwrap().ssr(), Some(&settings));
        assert_eq!(host.initial_scene.effects().unwrap().ssr(), Some(&settings));
        let next = settings_with_distance(settings);
        let effects = host
            .scene
            .effects()
            .unwrap()
            .clone()
            .with_ssr(next)
            .unwrap();
        host.apply(crate::RetainedUpdate::Settings(Box::new(effects)))
            .unwrap();
        assert_eq!(host.scene.effects().unwrap().ssr(), Some(&next));
    }

    fn settings_with_distance(settings: SceneSsr) -> SceneSsr {
        SceneSsr {
            max_distance_millimeters: 12_000,
            ..settings
        }
    }

    #[test]
    fn invalid_or_non_hdr_startup_settings_fail_before_rendering() {
        let mut options = crate::tests::options();
        options.ssr = Some(SceneSsr::default());
        assert!(startup_scene(crate::tests::scene(false), &options).is_err());
        options.frame_profile = FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        };
        options.ssr.as_mut().unwrap().max_distance_millimeters = 0;
        assert!(startup_scene(crate::tests::scene(false), &options).is_err());
    }
}

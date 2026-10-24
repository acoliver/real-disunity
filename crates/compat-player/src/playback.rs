use crate::{
    Session,
    extraction::{AssetTable, Contribution},
    graphics::{Graphics, write_png},
};
use compat_kit::{
    RunRequest, WorkerReport,
    model::{Capture, Observation},
};
use serde_json::{Value, json};
use std::path::{Component, Path};
use worldrunner::scene::gpu::{Viewport, offscreen::RgbaImage};

pub(crate) struct Playback<'a> {
    pub root: &'a Path,
    pub request: &'a RunRequest,
    pub report: &'a mut WorkerReport,
    pub session: &'a mut Option<Session>,
    pub viewport: Viewport,
    pub desktop: bool,
    pub pending_capture: Option<Capture>,
    pub pending_unload: bool,
    assets: Option<AssetTable>,
    graphics: Option<Graphics>,
    glyphs: crate::ui::UiGlyphs,
    textures_dirty: bool,
}
impl<'a> Playback<'a> {
    pub fn new(
        root: &'a Path,
        request: &'a RunRequest,
        report: &'a mut WorkerReport,
        session: &'a mut Option<Session>,
    ) -> Result<Self, String> {
        let size: [u32; 2] = if request.metadata["viewport"].is_null() {
            [960, 720]
        } else {
            serde_json::from_value(request.metadata["viewport"].clone())
                .map_err(|error| error.to_string())?
        };
        Ok(Self {
            root,
            request,
            report,
            session,
            viewport: Viewport {
                width: size[0],
                height: size[1],
            },
            desktop: request.desktop,
            pending_capture: None,
            pending_unload: false,
            assets: None,
            graphics: None,
            glyphs: crate::ui::UiGlyphs::default(),
            textures_dirty: true,
        })
    }
    pub fn active(&mut self) -> Result<&mut Session, String> {
        self.session
            .as_mut()
            .ok_or("timeline action requires a loaded session".into())
    }
    pub fn action(&mut self, action: &Value) -> Result<(), String> {
        crate::trace::action(&self.request.artifact_root, action, None)?;
        let result = self.apply(action);
        crate::trace::action(&self.request.artifact_root, action, Some(&result))?;
        result
    }
    fn apply(&mut self, action: &Value) -> Result<(), String> {
        crate::strict::action_mode(
            crate::strict::selected(&self.request.metadata)?,
            action["action"].as_str().unwrap_or(""),
        )?;
        let frame = action["frame"]
            .as_u64()
            .ok_or("timeline action has no unsigned frame")?;
        let phase = action["phase"].as_str().unwrap_or("unspecified");
        match action["action"]
            .as_str()
            .ok_or("timeline entry has no action")?
        {
            "load" => {
                if self.session.is_some() {
                    return Err("timeline loaded an already loaded session".into());
                }
                *self.session = Some(Session::prepare(self.root, self.request)?);
                self.active()?.load()?;
            }
            "input" => {
                let keys = serde_json::from_value(action["keys"].clone())
                    .map_err(|error| error.to_string())?;
                let axes = action.get("axes").cloned().unwrap_or_else(|| json!({}));
                self.active()?.input(keys, axes)?;
            }
            "advance" => self
                .active()?
                .advance(action["dt"].as_f64().ok_or("advance has no delta")?)?,
            "advance_rational" => {
                let (num, den) = crate::strict::ratio(&action["dt"])?;
                self.active()?.advance_rational(num, den)?;
            }
            "input_events" => self.active()?.input_events(&action["events"])?,
            "observe" => {
                let selector = action["selector"]
                    .as_str()
                    .ok_or("observation has no selector")?;
                let value = self.active()?.observe(selector)?;
                self.report.observations.push(Observation {
                    selector: selector.into(),
                    value,
                    frame,
                    phase: phase.into(),
                });
            }
            "capture" => self.capture(action, frame, phase)?,
            "reload" => self.active()?.reload()?,
            "unload" if self.desktop => self.pending_unload = true,
            "unload" => self.active()?.unload()?,
            value => return Err(format!("unsupported timeline action {value}")),
        }
        Ok(())
    }
    fn capture(&mut self, action: &Value, frame: u64, phase: &str) -> Result<(), String> {
        let path = action["path"].as_str().ok_or("capture has no path")?;
        safe_relative(path)?;
        let capture = Capture {
            path: path.into(),
            frame,
            phase: phase.into(),
            origin: self.report.evidence_label.clone(),
        };
        if self.desktop {
            if self.pending_capture.is_some() {
                return Err("one presented desktop frame supports one capture action".into());
            }
            self.pending_capture = Some(capture);
        } else {
            let contribution = self.contribution()?;
            if self.graphics.is_none() {
                self.graphics = Some(Graphics::new(
                    self.assets.as_ref().ok_or("assets were not extracted")?,
                    self.viewport,
                )?);
            }
            if self.textures_dirty {
                self.graphics
                    .as_mut()
                    .ok_or("graphics was not initialized")?
                    .textures(self.assets.as_ref().ok_or("assets were not initialized")?)?;
                self.textures_dirty = false;
            }
            let image = self
                .graphics
                .as_mut()
                .ok_or("graphics was not initialized")?
                .render(&contribution)?;
            self.save(capture, &image)?;
        }
        Ok(())
    }
    pub fn contribution(&mut self) -> Result<Contribution, String> {
        use worldrunner::scene::viewer::cpu_profile::CpuProfile;
        let _span = CpuProfile::span("contribution");
        if self.assets.is_none() {
            let _span = CpuProfile::span("asset_load");
            let session = self
                .session
                .as_ref()
                .ok_or("scene extraction requires a live world")?;
            self.assets = Some(AssetTable::load(&self.request.source_root, &session.graph)?);
        }
        let viewport = self.viewport;
        self.active()?.viewport([viewport.width, viewport.height])?;
        let start = std::time::Instant::now();
        let span = CpuProfile::span("snapshot");
        let snapshot = self.active()?.observe("world.snapshot")?;
        drop(span);
        let snapshot_done = start.elapsed();
        let span = CpuProfile::span("ui_layout");
        let layout = self.active()?.request(json!({"op":"ui.layout"}))?;
        drop(span);
        let layout_done = start.elapsed();
        let assets = self.assets.as_mut().ok_or("assets were not initialized")?;
        let span = CpuProfile::span("sprite_extract");
        let mut contribution = assets.extract(&snapshot, self.viewport)?;
        drop(span);
        let extract_done = start.elapsed();
        let span = CpuProfile::span("glyph_extract");
        self.textures_dirty |= self.glyphs.append(
            &layout,
            &snapshot,
            &contribution.view,
            &mut assets.textures,
            &mut contribution.quads,
        )?;
        drop(span);
        CpuProfile::detail("contribution_counts", || {
            format!(
                "quads={} textures={}",
                contribution.quads.len(),
                assets.textures.len()
            )
        });
        profile_extraction(
            &self.request.artifact_root,
            [snapshot_done, layout_done, extract_done, start.elapsed()],
            contribution.quads.len(),
            std::env::var_os("COMPAT_PROFILE_EXTRACTION").is_some(),
        )?;
        Ok(contribution)
    }
    pub fn textures_changed(&mut self) -> bool {
        std::mem::take(&mut self.textures_dirty)
    }
    pub fn textures(&self) -> Result<Vec<worldrunner::scene::plan::SceneTexture>, String> {
        Ok(self
            .assets
            .as_ref()
            .ok_or("assets were not initialized")?
            .textures
            .clone())
    }
    pub fn save(&mut self, capture: Capture, image: &RgbaImage) -> Result<(), String> {
        write_png(&self.request.artifact_root.join(&capture.path), image)?;
        self.report.captures.push(capture);
        Ok(())
    }
}
pub(crate) fn profile_extraction(
    root: &Path,
    times: [std::time::Duration; 4],
    quads: usize,
    enabled: bool,
) -> Result<(), String> {
    use std::io::Write;
    if !enabled {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("extraction-profile.ndjson"))
        .map_err(|error| error.to_string())?;
    let [snapshot, layout, extract, glyphs] = times.map(|time| time.as_secs_f64() * 1000.);
    let row = json!({"snapshot_ms":snapshot,"layout_ms":layout-snapshot,
        "extract_ms":extract-layout,"glyphs_ms":glyphs-extract,"quads":quads});
    writeln!(file, "{row}").map_err(|error| error.to_string())
}

fn safe_relative(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("capture output must be a relative path without traversal".into());
    }
    Ok(())
}

//! The content-tool plugin seam.
//!
//! Engine-generic verbs live in the content binary's own dispatch. Game-owned
//! verbs (dataset-specific import flows, shipped-asset governance) belong to
//! the game crate; a game links its tool behind a cargo feature, and the
//! binary consults this registry before admitting a verb is unknown.
//!
//! This crate has no dependencies so both the engine content crate and any
//! game crate can link it without dragging GPU or dataset crates along.

use std::error::Error;
use std::ffi::OsString;

/// A game's content-tool verbs, registered into the engine tool binary.
pub trait ContentTool: Send + Sync {
    /// Unique tool name for dispatch diagnostics.
    fn name(&self) -> &'static str;

    /// CLI verbs this tool owns; dispatch hands them over verbatim.
    fn verbs(&self) -> &'static [&'static str];

    /// Executes one of `verbs()` with the arguments that followed it.
    fn run(&self, verb: &str, arguments: &[OsString]) -> Result<(), Box<dyn Error>>;
}

/// The set of game plugins a binary was built with.
#[derive(Default)]
pub struct ContentToolRegistry {
    tools: Vec<Box<dyn ContentTool>>,
}

impl ContentToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn ContentTool>) {
        self.tools.push(tool);
    }

    /// True when any registered plugin owns `verb`.
    pub fn owns(&self, verb: &str) -> bool {
        self.tools.iter().any(|tool| tool.verbs().contains(&verb))
    }

    /// Every plugin verb, for the usage string.
    pub fn verbs(&self) -> Vec<&'static str> {
        let mut verbs: Vec<&'static str> = self
            .tools
            .iter()
            .flat_map(|tool| tool.verbs().iter().copied())
            .collect();
        verbs.sort_unstable();
        verbs
    }

    /// Runs `verb` on the plugin that owns it. Fails when no plugin owns it,
    /// so an unregistered verb stays a usage error instead of a silent pass.
    pub fn run(&self, verb: &str, arguments: &[OsString]) -> Result<bool, Box<dyn Error>> {
        for tool in &self.tools {
            if tool.verbs().contains(&verb) {
                tool.run(verb, arguments)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubTool {
        verbs: &'static [&'static str],
    }

    impl ContentTool for StubTool {
        fn name(&self) -> &'static str {
            "stub"
        }

        fn verbs(&self) -> &'static [&'static str] {
            self.verbs
        }

        fn run(&self, verb: &str, _arguments: &[OsString]) -> Result<(), Box<dyn Error>> {
            if verb == "fail" {
                Err("stub failure".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn plugin_verbs_dispatch_and_report_owned() {
        let mut registry = ContentToolRegistry::new();
        registry.register(Box::new(StubTool {
            verbs: &["verify-content"],
        }));
        assert!(registry.owns("verify-content"));
        assert!(!registry.owns("build"));
        assert_eq!(registry.verbs(), vec!["verify-content"]);
        assert!(
            registry
                .run("verify-content", &[])
                .expect("registered verb runs")
        );
    }

    #[test]
    fn unknown_verbs_do_not_dispatch() {
        let mut registry = ContentToolRegistry::new();
        registry.register(Box::new(StubTool { verbs: &[] }));
        let dispatched = registry
            .run("import-nothing", &[])
            .expect("unknown verb is a soft miss, not an error");
        assert!(!dispatched);
    }

    #[test]
    fn plugin_failures_surface() {
        let mut registry = ContentToolRegistry::new();
        registry.register(Box::new(StubTool { verbs: &["fail"] }));
        let error = registry.run("fail", &[]).unwrap_err();
        assert_eq!(error.to_string(), "stub failure");
    }
}

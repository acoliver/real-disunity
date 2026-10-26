use crate::{ComponentDesc, World};
impl World {
    pub fn active_audio_listeners(&self) -> usize {
        self.components
            .iter(self.identity)
            .map(|(_, c)| c)
            .filter(|c| {
                !self.lifetime.components.contains(&c.handle)
                    && !self.is_unpublished(c.object)
                    && matches!(c.desc, ComponentDesc::AudioListener { enabled: true })
                    && self
                        .active_in_hierarchy(c.object)
                        .expect("live component owner")
            })
            .count()
    }
}

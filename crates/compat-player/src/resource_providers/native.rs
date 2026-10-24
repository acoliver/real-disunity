use serde_json::{Value, json};
use worldrunner::managed::CompatibilityHost;

pub(super) struct Fixture {
    world: u64,
    authority: Value,
    context: Value,
    pub object: Value,
    pub component: Value,
}
pub(super) fn request(
    host: &CompatibilityHost,
    world: u64,
    value: &Value,
) -> Result<Value, String> {
    host.request(world, value).map_err(|e| e.to_string())
}
impl Fixture {
    pub fn open(host: &CompatibilityHost, world: u64) -> Result<Self, String> {
        let opened = request(
            host,
            world,
            &json!({"op":"session.open","profile":"native_phase_context_v1"}),
        )?;
        let authority = json!({"session":opened["session"],"host_key":opened["host_key"]});
        let context = request(host, world, &json!({"op":"phase.open","host":authority,"phase":"Initialize","frame":0,"fixed_tick":0}))?["context"].clone();
        request(
            host,
            world,
            &json!({"op":"phase.claim","phase":"Initialize","context":context}),
        )?;
        let mut fixture = Self {
            world,
            authority,
            context,
            object: Value::Null,
            component: Value::Null,
        };
        let camera = fixture.mutate(
            host,
            &json!({"op":"object.create","name":"resource-camera"}),
        )?["handle"]
            .clone();
        fixture.mutate(
            host,
            &json!({"op":"transform.set","object":camera,"position":[0.,0.,-10.]}),
        )?;
        fixture.mutate(host, &json!({"op":"component.add","object":camera,"kind":"Camera","fields":{"orthographic":true,"orthographic_size":2.,"near":0.1,"far":100.,"background":[0.,0.,0.,1.]}}))?;
        fixture.object = fixture.mutate(
            host,
            &json!({"op":"object.create","name":"authored-snake-square"}),
        )?["handle"]
            .clone();
        fixture.component = fixture.mutate(host, &json!({"op":"component.add","object":fixture.object,"kind":"SpriteRenderer","fields":{"sprite":{"$builtin":super::catalog::SPRITE},"color":[1.,1.,1.,1.],"sorting_layer":0,"sorting_order":0}}))?["handle"].clone();
        Ok(fixture)
    }
    pub fn mutate(&self, host: &CompatibilityHost, value: &Value) -> Result<Value, String> {
        request(host, self.world, value)
    }
    pub fn finish(&self, host: &CompatibilityHost) -> Result<(), String> {
        request(
            host,
            self.world,
            &json!({"op":"phase.finish","host":self.authority,"context":self.context}),
        )?;
        request(
            host,
            self.world,
            &json!({"op":"session.close","host":self.authority}),
        )?;
        Ok(())
    }
}

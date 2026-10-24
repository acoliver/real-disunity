use serde_json::{Value, json};
use worldrunner_disunity_ffi::{abi::*, response::*};
pub const PROFILE: &str = "transport_byte_limits_v1";
pub const REPLY: usize = 8 << 20;
pub const OUTSTANDING: usize = 16 << 20;

pub struct World(pub u64);
impl World {
    pub fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { disunity_world_create(&mut handle) }.code, 0);
        Self(handle)
    }
    pub fn call(&self, request: Value) -> Value {
        call(self.0, request)
    }
    pub fn select(&self) -> Value {
        self.call(json!({"version":1,"op":"transport.select","transport_profile":PROFILE}))
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}
pub fn call(world: u64, request: Value) -> Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    let handle = request_bytes(world, &bytes).unwrap();
    let output = response_bytes(handle).unwrap();
    response_free(handle).unwrap();
    serde_json::from_slice(&output).unwrap()
}

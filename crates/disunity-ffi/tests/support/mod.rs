use serde_json::{Value, json};
use worldrunner_disunity_ffi::abi::*;

pub struct World(pub u64);
impl World {
    pub fn new() -> Self {
        let mut world = 0;
        assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
        Self(world)
    }
    pub fn call(&self, value: Value) -> Value {
        call(self.0, &serde_json::to_vec(&value).unwrap())
    }
    pub fn snapshot(&self) -> Value {
        self.call(json!({"op":"snapshot"}))["data"].clone()
    }
}
impl Drop for World {
    fn drop(&mut self) {
        assert_eq!(unsafe { disunity_world_destroy(self.0) }.code, 0);
    }
}
pub fn submit(world: u64, bytes: &[u8]) -> (i32, u64) {
    let mut response = 0;
    let code =
        unsafe { disunity_world_request(world, bytes.as_ptr(), bytes.len(), &mut response) }.code;
    (code, response)
}
pub fn copy(response: u64) -> Vec<u8> {
    let mut size = 0;
    assert_eq!(
        unsafe { disunity_response_copy(response, std::ptr::null_mut(), 0, &mut size) }.code,
        2
    );
    let mut bytes = vec![0xA5; size];
    assert_eq!(
        unsafe { disunity_response_copy(response, bytes.as_mut_ptr(), size - 1, &mut size) }.code,
        2
    );
    assert!(bytes.iter().all(|byte| *byte == 0xA5));
    for _ in 0..2 {
        assert_eq!(
            unsafe { disunity_response_copy(response, bytes.as_mut_ptr(), bytes.len(), &mut size) }
                .code,
            0
        );
    }
    bytes
}
pub fn call(world: u64, bytes: &[u8]) -> Value {
    let (code, response) = submit(world, bytes);
    assert_eq!(code, 0);
    let bytes = copy(response);
    assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
    serde_json::from_slice(&bytes).unwrap()
}

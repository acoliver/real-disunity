use super::*;

impl Session {
    pub(super) fn execute(
        &mut self,
        world: u64,
        envelope: protocol::Envelope,
        bytes: &[u8],
        admit: &mut impl FnMut(&[u8]) -> Result<(), i32>,
    ) -> Result<Vec<u8>, i32> {
        self.execute_admitted(world, envelope, bytes, &mut Callback(admit))
    }
}
struct Callback<'a, F>(&'a mut F);
impl<F: FnMut(&[u8]) -> Result<(), i32>> Admission for Callback<'_, F> {
    fn admit(&mut self, _owner: Owner, bytes: &[u8]) -> Result<(), i32> {
        self.0(bytes)
    }
}

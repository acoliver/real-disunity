type Input = (&'static str, &'static [u8]);

#[path = "compiled_inputs_0.rs"]
mod part_0;
#[path = "compiled_inputs_1.rs"]
mod part_1;
#[path = "compiled_inputs_2.rs"]
mod part_2;

pub(super) fn inputs() -> impl Iterator<Item = &'static Input> {
    [part_0::INPUTS, part_1::INPUTS, part_2::INPUTS]
        .into_iter()
        .flatten()
}

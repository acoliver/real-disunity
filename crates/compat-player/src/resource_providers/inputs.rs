type Input = (&'static str, &'static [u8]);
#[path = "inputs_0.rs"]
mod inputs_0;
#[path = "inputs_1.rs"]
mod inputs_1;
#[path = "inputs_10.rs"]
mod inputs_10;
#[path = "inputs_11.rs"]
mod inputs_11;
#[path = "inputs_2.rs"]
mod inputs_2;
#[path = "inputs_3.rs"]
mod inputs_3;
#[path = "inputs_4.rs"]
mod inputs_4;
#[path = "inputs_5.rs"]
mod inputs_5;
#[path = "inputs_6.rs"]
mod inputs_6;
#[path = "inputs_7.rs"]
mod inputs_7;
#[path = "inputs_8.rs"]
mod inputs_8;
#[path = "inputs_9.rs"]
mod inputs_9;
pub(super) fn all() -> impl Iterator<Item = &'static Input> {
    [
        inputs_0::INPUTS,
        inputs_1::INPUTS,
        inputs_2::INPUTS,
        inputs_3::INPUTS,
        inputs_4::INPUTS,
        inputs_5::INPUTS,
        inputs_6::INPUTS,
        inputs_7::INPUTS,
        inputs_8::INPUTS,
        inputs_9::INPUTS,
        inputs_10::INPUTS,
        inputs_11::INPUTS,
    ]
    .into_iter()
    .flatten()
}

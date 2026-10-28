use worldrunner_unity_import::shader_prep::{Backend, check_with_naga, prepare};

fn op(code: u32, operands: &[u32]) -> Vec<u32> {
    let len = u32::try_from(operands.len() + 1).unwrap();
    [vec![(len << 16) | code], operands.to_vec()].concat()
}

/// A fragment shader writing a colour, with `view_index` made from the multiview view index.
/// IDs: 1 main, 2 output, 4 void, 5 function type, 7 float, 8 vec4, 14 output pointer,
/// 20 label, 29 constant colour, 31 uint, 60 view index, 61 its pointer, 62-64 the colour.
fn fragment(view_index: bool) -> Vec<u32> {
    let interface: &[u32] = if view_index { &[2, 60] } else { &[2] };
    let mut words = op(17, &[1]);
    if view_index {
        words.extend(op(17, &[4439]));
    }
    words.extend(
        [
            op(14, &[0, 1]),
            op(15, &[[4, 1, 0x6e69_616d, 0].as_slice(), interface].concat()),
            op(16, &[1, 7]),
            op(71, &[2, 30, 0]),
            op(71, &[60, 11, 4440]),
            op(19, &[4]),
            op(33, &[5, 4]),
            op(22, &[7, 32]),
            op(23, &[8, 7, 4]),
            op(21, &[31, 32, 0]),
            op(32, &[14, 3, 8]),
            op(32, &[61, 1, 31]),
            op(59, &[14, 2, 3]),
            op(59, &[61, 60, 1]),
            op(43, &[7, 17, 0]),
            op(44, &[8, 29, 17, 17, 17, 17]),
            op(54, &[4, 1, 0, 5]),
            op(248, &[20]),
        ]
        .concat(),
    );
    let colour = if view_index {
        let read = [op(61, &[31, 62, 60]), op(112, &[7, 63, 62])].concat();
        words.extend([read, op(80, &[8, 64, 63, 63, 63, 63])].concat());
        64
    } else {
        29
    };
    words.extend([op(62, &[2, colour]), op(253, &[]), op(56, &[])].concat());
    [vec![0x0723_0203, 0x0001_0000, 0, 65, 0], words].concat()
}

/// What each backend makes of a module, as `None` when naga writes it and otherwise the
/// failure's cause.
fn written(words: &[u32]) -> Vec<(Backend, Option<String>)> {
    let naga = check_with_naga(&prepare(words).unwrap().words).unwrap();
    Backend::ALL
        .into_iter()
        .map(|backend| (backend, backend.write(&naga).err().map(|e| e.cause())))
        .collect()
}

#[test]
fn writes_a_plain_module_for_every_backend() {
    let all = Backend::ALL.map(|backend| (backend, None));
    assert_eq!(written(&fragment(false)), all);
    assert_eq!(Backend::ALL.map(Backend::name), ["SPIR-V", "MSL", "HLSL"]);
}

#[test]
fn names_the_backend_that_cannot_write_a_module() {
    // HLSL shader model 5.1 has no SV_ViewID; SPIR-V and Metal can read the view index.
    assert_eq!(
        written(&fragment(true)),
        [
            (Backend::SpirV, None),
            (Backend::Msl, None),
            (Backend::Hlsl, Some("HLSL ShaderModelTooLow".to_owned())),
        ]
    );
}

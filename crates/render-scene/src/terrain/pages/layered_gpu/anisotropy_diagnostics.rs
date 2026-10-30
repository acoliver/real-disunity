use super::LayeredGpuSource;
use crate::device::GpuContext;
use std::fmt::Write;

pub(super) fn report(
    context: &GpuContext,
    sources: &[LayeredGpuSource],
    positions: &[[f32; 2]],
    samples: [&[u8]; 3],
) -> String {
    let adapter = context.adapter();
    let mut text = format!(
        "\nadapter={:?}\nadapter features={:?}\ndevice features={:?}\ndownlevel={:?}\npositions={positions:?}\nworld gradients: grazing dx=[0.00390625, 0], dy=[0, 0.0625]; baseline dx=[0.00390625, 0], dy=[0, 0.00390625]\nsource UV: (world + [0.13, -0.07]) / 1, rotation=0; 1x grazing shader gradients=[0.0625, 0], [0, 0.0625]; 16x uses 16 centred explicit-level taps along dy at LOD 0\n",
        adapter.get_info(),
        adapter.features(),
        context.device().features(),
        adapter.get_downlevel_capabilities(),
    );
    for (label, bytes) in ["1x grazing", "16x grazing", "1x isotropic baseline"]
        .into_iter()
        .zip(samples)
    {
        let values: Vec<[u8; 16]> = bytes
            .chunks_exact(16)
            .map(|v| v.try_into().unwrap())
            .collect();
        writeln!(text, "{label} raw packed records={values:?}").unwrap();
    }
    for source in sources {
        let resident = source.resident(context.device(), context.queue()).unwrap();
        writeln!(
            text,
            "requested anisotropy={} actual created sampler descriptors={:?}",
            f32::from_le_bytes(source.parameters[28..32].try_into().unwrap()) + 1.0,
            resident.sampler_descriptors
        )
        .unwrap();
    }
    text
}

use worldrunner_render_plan::SceneMaterialChannels;

fn components(channels: SceneMaterialChannels) -> [worldrunner_render_plan::TextureChannel; 4] {
    [
        channels.roughness,
        channels.metalness,
        channels.ambient_occlusion,
        channels.self_illumination,
    ]
}

// Integer offsets keep default uniform bytes zero without arithmetic on sampled values.
pub(super) fn encode(
    bytes: &mut [u8],
    channels: SceneMaterialChannels,
    neutral: SceneMaterialChannels,
) {
    for ((slot, channel), baseline) in bytes
        .chunks_exact_mut(16)
        .zip(components(channels))
        .zip(components(neutral))
    {
        slot[..4].copy_from_slice(&(channel as i32 - baseline as i32).to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_plan::TextureChannel;

    #[test]
    fn zero_neutral_offsets_reconstruct_exact_component_indices() {
        for baseline in [
            SceneMaterialChannels::default(),
            SceneMaterialChannels::PACKED_IMPOSTOR,
        ] {
            let mut bytes = [0; 64];
            encode(&mut bytes, baseline, baseline);
            assert_eq!(bytes, [0; 64]);
            for channel in [
                TextureChannel::R,
                TextureChannel::G,
                TextureChannel::B,
                TextureChannel::A,
            ] {
                let chosen = SceneMaterialChannels {
                    roughness: channel,
                    metalness: channel,
                    ambient_occlusion: channel,
                    self_illumination: channel,
                };
                encode(&mut bytes, chosen, baseline);
                for (slot, neutral) in bytes.chunks_exact(16).zip(components(baseline)) {
                    let offset = i32::from_le_bytes(slot[..4].try_into().unwrap());
                    assert_eq!(offset + neutral as i32, channel as i32);
                    assert_eq!(slot[4..], [0; 12]);
                }
            }
        }
    }
}

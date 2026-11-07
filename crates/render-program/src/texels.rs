//! Counts over the texels of an image read back, for a report that must not hold the image: how
//! many texels hold a value other than zero and the range of the values in each channel.

/// The values of one channel of an image.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelStats {
    /// Texels whose value in this channel is not zero.
    pub nonzero: usize,
    pub min: f32,
    pub max: f32,
}

/// What an image holds, by count.
#[derive(Clone, Debug, PartialEq)]
pub struct TexelStats {
    pub texels: usize,
    /// Texels with a value other than zero in any channel. For a format this module does not
    /// decode, texels with any byte other than zero.
    pub nonzero: usize,
    /// One entry for each channel of a decoded format, empty for any other.
    pub channels: Vec<ChannelStats>,
}

impl TexelStats {
    /// The least and the greatest value over all channels, when the format was decoded.
    #[must_use]
    pub fn range(&self) -> Option<(f32, f32)> {
        let min = self.channels.iter().map(|c| c.min).reduce(f32::min)?;
        let max = self.channels.iter().map(|c| c.max).reduce(f32::max)?;
        Some((min, max))
    }

    /// The counts in words, for a report.
    #[must_use]
    pub fn describe(&self) -> String {
        let range = self.range().map_or_else(
            || "not decoded".to_owned(),
            |(min, max)| format!("[{min}, {max}]"),
        );
        format!(
            "{} texels, {} non-zero, values {range}",
            self.texels, self.nonzero
        )
    }
}

/// How the channels of a format are stored.
#[derive(Clone, Copy)]
enum Channel {
    Unorm8,
    Unorm16,
    Half,
    Float,
}

impl Channel {
    fn bytes(self) -> usize {
        match self {
            Self::Unorm8 => 1,
            Self::Unorm16 | Self::Half => 2,
            Self::Float => 4,
        }
    }

    fn value(self, bytes: &[u8]) -> f32 {
        match self {
            Self::Unorm8 => f32::from(bytes[0]) / 255.0,
            Self::Unorm16 => f32::from(u16::from_le_bytes([bytes[0], bytes[1]])) / 65535.0,
            Self::Half => half(u16::from_le_bytes([bytes[0], bytes[1]])),
            Self::Float => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }
}

fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    match exponent {
        0 => sign * fraction * 2f32.powi(-24),
        31 if bits.trailing_zeros() >= 10 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}

fn layout(format: wgpu::TextureFormat) -> Option<(Channel, usize)> {
    use wgpu::TextureFormat as F;
    Some(match format {
        F::R8Unorm => (Channel::Unorm8, 1),
        F::Rg8Unorm => (Channel::Unorm8, 2),
        F::Rgba8Unorm | F::Rgba8UnormSrgb | F::Bgra8Unorm | F::Bgra8UnormSrgb => {
            (Channel::Unorm8, 4)
        }
        F::R16Unorm => (Channel::Unorm16, 1),
        F::Rg16Unorm => (Channel::Unorm16, 2),
        F::Rgba16Unorm => (Channel::Unorm16, 4),
        F::R16Float => (Channel::Half, 1),
        F::Rg16Float => (Channel::Half, 2),
        F::Rgba16Float => (Channel::Half, 4),
        F::R32Float => (Channel::Float, 1),
        F::Rg32Float => (Channel::Float, 2),
        F::Rgba32Float => (Channel::Float, 4),
        _ => return None,
    })
}

/// Counts the texels of `bytes`, tightly packed rows of `format`.
#[must_use]
pub fn texel_stats(bytes: &[u8], format: wgpu::TextureFormat) -> TexelStats {
    let Some((channel, count)) = layout(format) else {
        let texel = format.block_copy_size(None).map_or(1, |size| size as usize);
        let texels = bytes.chunks_exact(texel);
        return TexelStats {
            texels: texels.len(),
            nonzero: texels
                .filter(|texel| texel.iter().any(|byte| *byte != 0))
                .count(),
            channels: Vec::new(),
        };
    };
    let width = channel.bytes();
    let mut channels = vec![
        ChannelStats {
            nonzero: 0,
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
        };
        count
    ];
    let mut texels = 0;
    let mut nonzero = 0;
    for texel in bytes.chunks_exact(width * count) {
        texels += 1;
        let mut any = false;
        for (stats, bytes) in channels.iter_mut().zip(texel.chunks_exact(width)) {
            let value = channel.value(bytes);
            stats.min = stats.min.min(value);
            stats.max = stats.max.max(value);
            if value != 0.0 {
                stats.nonzero += 1;
                any = true;
            }
        }
        nonzero += usize::from(any);
    }
    TexelStats {
        texels,
        nonzero,
        channels,
    }
}

/// How far a decoded channel may be from the value a texel was cleared to and still count as
/// that value: more than the step of an 8-bit channel.
const CLEARED_WITHIN: f32 = 0.005;

/// Counts the texels of `bytes`, tightly packed rows of `format`, with a channel that is not the
/// value in `cleared` (indexed by channel, as many as the format has) to within [`CLEARED_WITHIN`].
/// A render that cleared its target to `cleared` and wrote nothing leaves none. `None` for a
/// format this module does not decode.
#[must_use]
pub fn texels_differing(
    bytes: &[u8],
    format: wgpu::TextureFormat,
    cleared: [f32; 4],
) -> Option<usize> {
    let (channel, count) = layout(format)?;
    let width = channel.bytes();
    Some(
        bytes
            .chunks_exact(width * count)
            .filter(|texel| {
                texel
                    .chunks_exact(width)
                    .zip(cleared)
                    .any(|(bytes, expected)| {
                        (channel.value(bytes) - expected).abs() > CLEARED_WITHIN
                    })
            })
            .count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn halves(values: &[u16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn half_floats_are_counted_and_ranged_by_their_values() {
        // 0.0, 0.5, 2.0, negative zero: two texels hold a value.
        let stats = texel_stats(
            &halves(&[0x0000, 0x3800, 0x4000, 0x8000]),
            wgpu::TextureFormat::R16Float,
        );
        assert_eq!((stats.texels, stats.nonzero), (4, 2));
        assert_eq!(stats.range(), Some((0.0, 2.0)));
        assert_eq!(stats.channels[0].nonzero, 2);
    }

    #[test]
    fn a_texel_is_non_zero_when_any_channel_is_and_each_channel_is_counted() {
        // Three RGBA8 texels: (0, 0, 0, 255), (0, 0, 0, 0) and (51, 0, 0, 0).
        let stats = texel_stats(
            &[0, 0, 0, 255, 0, 0, 0, 0, 51, 0, 0, 0],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        assert_eq!((stats.texels, stats.nonzero), (3, 2));
        let counts: Vec<_> = stats.channels.iter().map(|c| c.nonzero).collect();
        assert_eq!(counts, [1, 0, 0, 1]);
        assert!((stats.channels[0].max - 0.2).abs() < 1e-6);
        assert_eq!(stats.range(), Some((0.0, 1.0)));
    }

    #[test]
    fn floats_and_wide_integers_are_decoded() {
        let floats: Vec<u8> = [0.0f32, -1.5, 3.25]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let stats = texel_stats(&floats, wgpu::TextureFormat::R32Float);
        assert_eq!((stats.texels, stats.nonzero), (3, 2));
        assert_eq!(stats.range(), Some((-1.5, 3.25)));
        let stats = texel_stats(&halves(&[0, 65535]), wgpu::TextureFormat::R16Unorm);
        assert_eq!((stats.texels, stats.nonzero), (2, 1));
        assert_eq!(stats.range(), Some((0.0, 1.0)));
    }

    #[test]
    fn a_format_that_is_not_decoded_counts_texels_with_any_byte_set() {
        let stats = texel_stats(&[0, 0, 0, 0, 0, 1, 0, 0], wgpu::TextureFormat::R32Uint);
        assert_eq!((stats.texels, stats.nonzero), (2, 1));
        assert!(stats.channels.is_empty());
        assert_eq!(stats.range(), None);
    }

    #[test]
    fn texels_differing_from_the_clear_are_the_ones_a_render_wrote_to() {
        // Two RGBA8 texels: the clear (0.25, 0.5, 0.75, 1.0) and one written with zeros.
        let clear = [0.25, 0.5, 0.75, 1.0];
        let bytes = [64, 128, 191, 255, 0, 0, 0, 0];
        assert_eq!(
            texels_differing(&bytes, wgpu::TextureFormat::Rgba8Unorm, clear),
            Some(1)
        );
        // A texel that differs in one channel only counts, once.
        let bytes = [64, 128, 191, 0, 64, 128, 191, 255];
        assert_eq!(
            texels_differing(&bytes, wgpu::TextureFormat::Rgba8Unorm, clear),
            Some(1)
        );
        // A one-channel format compares its channel with the first value.
        assert_eq!(
            texels_differing(
                &halves(&[0x3400, 0x0000]),
                wgpu::TextureFormat::R16Float,
                clear
            ),
            Some(1)
        );
        assert_eq!(
            texels_differing(&[0; 4], wgpu::TextureFormat::R32Uint, clear),
            None
        );
    }
}

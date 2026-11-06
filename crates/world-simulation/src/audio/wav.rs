use super::{AudioError, MAX_WAV_BYTES, validate_rate};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Clip {
    rate: u32,
    samples: Arc<[[f32; 2]]>,
}
impl Clip {
    /// Parses little-endian RIFF PCM WAV. Float samples are clamped to [-1, 1].
    /// Validates size, chunk extents and format before allocating decoded storage.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, AudioError> {
        if bytes.len() > MAX_WAV_BYTES {
            return Err(AudioError::ClipTooLarge);
        }
        if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
            return Err(AudioError::MalformedWav);
        }
        let size = usize::try_from(read_u32(bytes, 4)?).map_err(|_| AudioError::MalformedWav)?;
        if size.checked_add(8) != Some(bytes.len()) {
            return Err(AudioError::MalformedWav);
        }
        let (format, data) = chunks(bytes)?;
        let format = Format::parse(format)?;
        if data.is_empty() || data.len() % usize::from(format.align) != 0 {
            return Err(AudioError::MalformedWav);
        }
        let samples = data
            .chunks_exact(usize::from(format.align))
            .map(|frame| format.frame(frame))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            rate: format.rate,
            samples: samples.into(),
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
    pub fn samples(&self) -> &[[f32; 2]] {
        &self.samples
    }
}

fn chunks(bytes: &[u8]) -> Result<(&[u8], &[u8]), AudioError> {
    let mut cursor = 12_usize;
    let (mut format, mut data) = (None, None);
    while cursor < bytes.len() {
        let tag = bytes
            .get(cursor..cursor + 4)
            .ok_or(AudioError::MalformedWav)?;
        let size =
            usize::try_from(read_u32(bytes, cursor + 4)?).map_err(|_| AudioError::MalformedWav)?;
        let start = cursor + 8;
        let end = start.checked_add(size).ok_or(AudioError::MalformedWav)?;
        let chunk = bytes.get(start..end).ok_or(AudioError::MalformedWav)?;
        match tag {
            b"fmt " if format.replace(chunk).is_some() => return Err(AudioError::MalformedWav),
            b"data" if data.replace(chunk).is_some() => return Err(AudioError::MalformedWav),
            _ => {}
        }
        cursor = end.checked_add(size % 2).ok_or(AudioError::MalformedWav)?;
        if cursor > bytes.len() {
            return Err(AudioError::MalformedWav);
        }
    }
    Ok((
        format.ok_or(AudioError::MalformedWav)?,
        data.ok_or(AudioError::MalformedWav)?,
    ))
}
struct Format {
    tag: u16,
    channels: u16,
    rate: u32,
    align: u16,
    bits: u16,
}
impl Format {
    fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        let format = Self {
            tag: read_u16(bytes, 0)?,
            channels: read_u16(bytes, 2)?,
            rate: read_u32(bytes, 4)?,
            align: read_u16(bytes, 12)?,
            bits: read_u16(bytes, 14)?,
        };
        if !matches!((format.tag, format.bits), (1, 8 | 16 | 24) | (3, 32))
            || !(1..=2).contains(&format.channels)
        {
            return Err(AudioError::UnsupportedFormat);
        }
        validate_rate(format.rate)?;
        let align = format.channels * (format.bits / 8);
        if format.align != align || read_u32(bytes, 8)? != format.rate * u32::from(align) {
            return Err(AudioError::MalformedWav);
        }
        match bytes.len() {
            16 => {}
            18.. if usize::from(read_u16(bytes, 16)?) + 18 == bytes.len() => {}
            _ => return Err(AudioError::MalformedWav),
        }
        Ok(format)
    }
    fn frame(&self, bytes: &[u8]) -> Result<[f32; 2], AudioError> {
        let size = usize::from(self.bits / 8);
        let left = self.sample(&bytes[..size])?;
        let right = if self.channels == 2 {
            self.sample(&bytes[size..])?
        } else {
            left
        };
        Ok([left, right])
    }
    fn sample(&self, bytes: &[u8]) -> Result<f32, AudioError> {
        let value = match self.bits {
            8 => (f32::from(bytes[0]) - 128.0) / 128.0,
            16 => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
            24 => {
                f32::from(i16::from_le_bytes([bytes[1], bytes[2]])) / 32768.0
                    + f32::from(bytes[0]) / 8_388_608.0
            }
            32 => f32::from_le_bytes(bytes.try_into().map_err(|_| AudioError::MalformedWav)?),
            _ => unreachable!("validated encoding"),
        };
        if !value.is_finite() {
            return Err(AudioError::InvalidSample);
        }
        Ok(value.clamp(-1.0, 1.0))
    }
}
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, AudioError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(AudioError::MalformedWav)?
            .try_into()
            .map_err(|_| AudioError::MalformedWav)?,
    ))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, AudioError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(AudioError::MalformedWav)?
            .try_into()
            .map_err(|_| AudioError::MalformedWav)?,
    ))
}

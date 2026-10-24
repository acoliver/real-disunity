fn output(input: usize, expected: usize, channels: usize) -> Result<Vec<u8>, String> {
    if expected > 67_108_864
        || !expected.is_multiple_of(4)
        || (expected / 4).checked_mul(channels) != Some(input)
    {
        return Err("decoded pixel count differs from dimensions".into());
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(expected)
        .map_err(|error| error.to_string())?;
    rgba.resize(expected, 0);
    Ok(rgba)
}

pub(super) fn expand_rgb(bytes: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut rgba = output(bytes.len(), expected, 3)?;
    for (source, target) in bytes
        .as_chunks::<3>()
        .0
        .iter()
        .zip(rgba.as_chunks_mut::<4>().0)
    {
        *target = [source[0], source[1], source[2], 255];
    }
    Ok(rgba)
}

pub(super) fn expand_l8(bytes: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut rgba = output(bytes.len(), expected, 1)?;
    for (source, target) in bytes.iter().zip(rgba.as_chunks_mut::<4>().0) {
        *target = [*source, *source, *source, 255];
    }
    Ok(rgba)
}

pub(super) fn expand_la8(bytes: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut rgba = output(bytes.len(), expected, 2)?;
    for (source, target) in bytes
        .as_chunks::<2>()
        .0
        .iter()
        .zip(rgba.as_chunks_mut::<4>().0)
    {
        *target = [source[0], source[0], source[0], source[1]];
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansions_preserve_channels_alpha_and_row_order() {
        assert_eq!(
            expand_rgb(&[1, 2, 3, 4, 5, 6], 8).unwrap(),
            [1, 2, 3, 255, 4, 5, 6, 255]
        );
        assert_eq!(expand_l8(&[7, 9], 8).unwrap(), [7, 7, 7, 255, 9, 9, 9, 255]);
        assert_eq!(
            expand_la8(&[7, 0, 9, 128], 8).unwrap(),
            [7, 7, 7, 0, 9, 9, 9, 128]
        );
    }

    #[test]
    fn invalid_lengths_and_excessive_outputs_reject_before_allocation() {
        assert!(expand_rgb(&[1, 2], 4).is_err());
        assert!(expand_rgb(&[1, 2, 3], 8).is_err());
        assert!(expand_l8(&[1], 3).is_err());
        assert!(expand_la8(&[1], 4).is_err());
        assert!(expand_l8(&[], usize::MAX).is_err());
        assert!(expand_l8(&[], 67_108_868).is_err());
        assert!(expand_rgb(&[], 0).unwrap().is_empty());
    }
}

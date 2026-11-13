/// Proves simple glyph records complete and identifies contours with no area.
/// Composite and zero-contour records remain subject to the existing outline checks.
pub(super) fn degenerate(mut data: &[u8]) -> Option<bool> {
    let contours = i16::from_be_bytes(word(&mut data)?.to_be_bytes());
    if contours <= 0 {
        return Some(false);
    }
    data = data.get(8..)?;
    let mut endpoints = Vec::new();
    for _ in 0..contours {
        let endpoint = word(&mut data)?;
        if endpoints
            .last()
            .is_some_and(|&previous| endpoint <= previous)
        {
            return None;
        }
        endpoints.push(endpoint);
    }
    let points = usize::from(endpoints.last()?.checked_add(1)?);
    let instructions = usize::from(word(&mut data)?);
    data = data.get(instructions..)?;
    let flags = flags(&mut data, points)?;
    let x = coordinates(&mut data, &flags, 0x02, 0x10)?;
    let y = coordinates(&mut data, &flags, 0x04, 0x20)?;
    // Remaining bytes are glyph alignment padding, not additional point records.
    let mut start = 0;
    let mut empty = true;
    for endpoint in endpoints {
        let end = usize::from(endpoint) + 1;
        empty &= collinear(&x[start..end], &y[start..end]);
        start = end;
    }
    Some(empty)
}

fn flags(data: &mut &[u8], points: usize) -> Option<Vec<u8>> {
    let mut flags = Vec::with_capacity(points);
    while flags.len() < points {
        let flag = byte(data)?;
        let count = if flag & 0x08 == 0 {
            1
        } else {
            usize::from(byte(data)?) + 1
        };
        let end = flags.len() + count;
        if end > points {
            return None;
        }
        flags.resize(end, flag);
    }
    Some(flags)
}

fn coordinates(data: &mut &[u8], flags: &[u8], short: u8, same: u8) -> Option<Vec<i16>> {
    let mut coordinates = Vec::with_capacity(flags.len());
    let mut position = 0_i16;
    for flag in flags {
        let delta = match (flag & short != 0, flag & same != 0) {
            (true, positive) => {
                let value = i16::from(byte(data)?);
                if positive { value } else { -value }
            }
            (false, true) => 0,
            (false, false) => i16::from_be_bytes(word(data)?.to_be_bytes()),
        };
        // Match the outline parser's signed 16-bit coordinate accumulation.
        position = position.wrapping_add(delta);
        coordinates.push(position);
    }
    Some(coordinates)
}

fn collinear(x: &[i16], y: &[i16]) -> bool {
    let origin = (i64::from(x[0]), i64::from(y[0]));
    let Some(direction) = x.iter().zip(y).find(|&(&px, &py)| (px, py) != (x[0], y[0])) else {
        return true;
    };
    let dx = i64::from(*direction.0) - origin.0;
    let dy = i64::from(*direction.1) - origin.1;
    x.iter()
        .zip(y)
        .all(|(&x, &y)| dx * (i64::from(y) - origin.1) == dy * (i64::from(x) - origin.0))
}

fn word(data: &mut &[u8]) -> Option<u16> {
    let (value, tail) = data.split_first_chunk::<2>()?;
    *data = tail;
    Some(u16::from_be_bytes(*value))
}

fn byte(data: &mut &[u8]) -> Option<u8> {
    let (&value, tail) = data.split_first()?;
    *data = tail;
    Some(value)
}

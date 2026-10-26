// Synthetic triangle outlines and metrics, generated here without external font data.
pub fn font() -> Vec<u8> {
    font_with(1, 800, -200)
}

/// A font with 1000 units per em, `contours` triangle contours per glyph, and the given
/// vertical metrics, so its line height can differ from its em size.
pub fn font_with(contours: i16, ascent: i16, descent: i16) -> Vec<u8> {
    assemble(&[Vec::new(), glyph(contours)], ascent, descent)
}

/// A font of `glyphs`, in order, with 1000 units per em and the given vertical metrics.
pub fn assemble(glyphs: &[Vec<u8>], ascent: i16, descent: i16) -> Vec<u8> {
    let count = u16::try_from(glyphs.len()).unwrap();
    let mut head = vec![0; 54];
    head[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f_3cf5_u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000_u16.to_be_bytes());
    let mut hhea = vec![0; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&ascent.to_be_bytes());
    hhea[6..8].copy_from_slice(&descent.to_be_bytes());
    hhea[34..36].copy_from_slice(&count.to_be_bytes());
    let mut maxp = vec![0; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&count.to_be_bytes());
    let mut glyf = Vec::new();
    let mut loca = 0_u16.to_be_bytes().to_vec();
    for body in glyphs {
        glyf.extend(body);
        loca.extend(u16::try_from(glyf.len() / 2).unwrap().to_be_bytes());
    }
    let mut hmtx = Vec::new();
    for advance in [500_u16, 600]
        .into_iter()
        .chain(std::iter::repeat(300))
        .take(glyphs.len())
    {
        hmtx.extend(advance.to_be_bytes());
        hmtx.extend(0_u16.to_be_bytes());
    }
    let cmap = cmap();
    let tables = [
        (*b"cmap", cmap),
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"loca", loca),
        (*b"maxp", maxp),
    ];
    let mut out = vec![0; 12 + tables.len() * 16];
    out[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    out[4..6].copy_from_slice(&7_u16.to_be_bytes());
    for (i, (tag, bytes)) in tables.into_iter().enumerate() {
        let offset = u32::try_from(out.len()).unwrap();
        let length = u32::try_from(bytes.len()).unwrap();
        let record = 12 + i * 16;
        out[record..record + 4].copy_from_slice(&tag);
        out[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        out[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        out.extend(bytes);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    out
}

pub fn glyph(contours: i16) -> Vec<u8> {
    let mut glyf = Vec::new();
    for n in [contours, 0, 0, 500, 700] {
        glyf.extend(n.to_be_bytes());
    }
    for i in 0..contours {
        glyf.extend((i * 3 + 2).to_be_bytes());
    }
    glyf.extend(0_u16.to_be_bytes());
    glyf.extend(vec![1; usize::try_from(contours).unwrap() * 3]);
    for _ in 0..contours {
        for n in [0_i16, 500, -500] {
            glyf.extend(n.to_be_bytes());
        }
    }
    for i in 0..contours {
        for n in [if i == 0 { 0_i16 } else { -700 }, 0, 700] {
            glyf.extend(n.to_be_bytes());
        }
    }
    if !glyf.len().is_multiple_of(2) {
        glyf.push(0);
    }
    glyf
}

fn cmap() -> Vec<u8> {
    let mut cmap = Vec::new();
    for n in [0_u16, 1, 3, 1] {
        cmap.extend(n.to_be_bytes());
    }
    cmap.extend(12_u32.to_be_bytes());
    let count = 96_u16;
    for n in [4_u16, 16 + count * 8, 0, count * 2, 128, 6, count * 2 - 128] {
        cmap.extend(n.to_be_bytes());
    }
    for c in 32_u16..=126 {
        cmap.extend(c.to_be_bytes());
    }
    cmap.extend(u16::MAX.to_be_bytes());
    cmap.extend(0_u16.to_be_bytes());
    for c in 32_u16..=126 {
        cmap.extend(c.to_be_bytes());
    }
    cmap.extend(u16::MAX.to_be_bytes());
    for c in 32_u16..=126 {
        cmap.extend(1_u16.wrapping_sub(c).to_be_bytes());
    }
    cmap.extend(1_u16.to_be_bytes());
    cmap.extend(vec![0; usize::from(count) * 2]);
    cmap
}

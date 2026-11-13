use super::ui_font;

pub fn words(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn offset(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 2].copy_from_slice(&u16::try_from(value).unwrap().to_be_bytes());
}

pub struct Script {
    pub tag: [u8; 4],
    pub required: Option<u16>,
    pub features: Vec<u16>,
}
impl Script {
    pub fn default_kern(tag: [u8; 4]) -> Self {
        Self {
            tag,
            required: None,
            features: vec![0],
        }
    }
    fn bytes(&self) -> Vec<u8> {
        let mut out = words(&[
            4,
            0,
            0,
            self.required.unwrap_or(u16::MAX),
            u16::try_from(self.features.len()).unwrap(),
        ]);
        out.extend(words(&self.features));
        out
    }
}

pub struct Feature {
    pub tag: [u8; 4],
    pub lookups: Vec<u16>,
}
impl Feature {
    pub fn kern(lookups: Vec<u16>) -> Self {
        Self {
            tag: *b"kern",
            lookups,
        }
    }
    fn bytes(&self) -> Vec<u8> {
        let mut out = words(&[0, u16::try_from(self.lookups.len()).unwrap()]);
        out.extend(words(&self.lookups));
        out
    }
}

pub struct Lookup {
    pub kind: u16,
    pub subtables: Vec<Vec<u8>>,
}
impl Lookup {
    pub fn pair(format: u16, extension: bool) -> Self {
        Self::new(vec![pair(format, -100, false, false)], extension)
    }
    pub fn new(subtables: Vec<Vec<u8>>, extension: bool) -> Self {
        if extension {
            Self {
                kind: 9,
                subtables: subtables
                    .into_iter()
                    .map(|data| {
                        let mut out = words(&[1, 2, 0, 8]);
                        out.extend(data);
                        out
                    })
                    .collect(),
            }
        } else {
            Self { kind: 2, subtables }
        }
    }
    fn bytes(&self) -> Vec<u8> {
        let mut out = words(&[self.kind, 0, u16::try_from(self.subtables.len()).unwrap()]);
        out.resize(6 + self.subtables.len() * 2, 0);
        for (i, data) in self.subtables.iter().enumerate() {
            let at = out.len();
            offset(&mut out, 6 + 2 * i, at);
            out.extend(data);
        }
        out
    }
}

fn tagged_list(items: impl IntoIterator<Item = ([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let items: Vec<_> = items.into_iter().collect();
    let mut out = words(&[u16::try_from(items.len()).unwrap()]);
    out.resize(2 + items.len() * 6, 0);
    for (i, (tag, data)) in items.into_iter().enumerate() {
        let at = out.len();
        out[2 + i * 6..6 + i * 6].copy_from_slice(&tag);
        offset(&mut out, 6 + i * 6, at);
        out.extend(data);
    }
    out
}

pub fn gpos(scripts: &[Script], features: &[Feature], lookups: &[Lookup]) -> Vec<u8> {
    let mut out = words(&[1, 0, 10, 0, 0]);
    out.extend(tagged_list(scripts.iter().map(|s| (s.tag, s.bytes()))));
    let at = out.len();
    offset(&mut out, 6, at);
    out.extend(tagged_list(features.iter().map(|f| (f.tag, f.bytes()))));
    let at = out.len();
    offset(&mut out, 8, at);
    let mut list = words(&[u16::try_from(lookups.len()).unwrap()]);
    list.resize(2 + lookups.len() * 2, 0);
    for (i, lookup) in lookups.iter().enumerate() {
        let at = list.len();
        offset(&mut list, 2 + i * 2, at);
        list.extend(lookup.bytes());
    }
    out.extend(list);
    out
}

pub fn default_gpos(lookups: &[Lookup]) -> Vec<u8> {
    gpos(
        &[Script::default_kern(*b"DFLT")],
        &[Feature::kern(
            (0..u16::try_from(lookups.len()).unwrap()).collect(),
        )],
        lookups,
    )
}

fn record(out: &mut Vec<u8>, advance: i16, extras: bool, device_offsets: &mut Vec<usize>) {
    if !extras {
        out.extend(advance.to_be_bytes());
        return;
    }
    for values in [[37_i16, 41, advance, 43], [47, 53, 59, 61]] {
        out.extend(values.into_iter().flat_map(i16::to_be_bytes));
        for _ in 0..4 {
            device_offsets.push(out.len());
            out.extend([0, 0]);
        }
    }
}

/// Both formats cover A only. The normal matrix maps A/V to class 1, with zero elsewhere.
/// The class-zero matrix leaves all glyphs unclassified, including the covered A.
pub fn pair(format: u16, advance: i16, class_zero: bool, extras: bool) -> Vec<u8> {
    let (value1, value2) = if extras { (255, 255) } else { (4, 0) };
    let mut devices = Vec::new();
    let mut out = if format == 1 {
        words(&[1, 0, value1, value2, 1, 12, 1, 2])
    } else {
        let count = if class_zero { 1 } else { 2 };
        words(&[2, 0, value1, value2, 0, 0, count, count])
    };
    if format == 1 || class_zero {
        record(&mut out, advance, extras, &mut devices);
    } else {
        for value in [0, 0, 0, advance] {
            record(&mut out, value, extras, &mut devices);
        }
    }
    if format == 2 {
        for (header, glyph) in [(8, 1), (10, 2)] {
            let at = out.len();
            offset(&mut out, header, at);
            out.extend(if class_zero {
                words(&[2, 0])
            } else {
                words(&[1, glyph, 1, 1])
            });
        }
    }
    let at = out.len();
    offset(&mut out, 2, at);
    out.extend(words(&[1, 1, 1]));
    let at = out.len();
    for field in devices {
        offset(&mut out, field, at);
    }
    out.extend(words(&[20, 20, 1, 0x4000]));
    out
}

pub fn kern(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
    let count = u16::try_from(pairs.len()).unwrap();
    let mut out = words(&[0, 1, 0, 14 + count * 6, 1, count, 0, 0, 0]);
    for &(first, second, advance) in pairs {
        out.extend(words(&[first, second]));
        out.extend(advance.to_be_bytes());
    }
    out
}

pub fn font(gpos: Option<Vec<u8>>, legacy: Option<Vec<u8>>) -> Vec<u8> {
    let mapping: Vec<_> = (32..=255)
        .map(|c| (c, if c == 86 { 2 } else { 1 }))
        .collect();
    let mut tables = vec![
        (*b"cmap", ui_font::cmap_with(&mapping)),
        (*b"hmtx", words(&[500, 0, 600, 0, 600, 0])),
    ];
    if let Some(data) = gpos {
        tables.push((*b"GPOS", data));
    }
    if let Some(data) = legacy {
        tables.push((*b"kern", data));
    }
    ui_font::assemble_with(
        &[Vec::new(), ui_font::glyph(1), ui_font::glyph(1)],
        800,
        -200,
        tables,
    )
}

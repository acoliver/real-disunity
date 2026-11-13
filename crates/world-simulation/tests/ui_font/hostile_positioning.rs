use super::positioning::{Feature, Lookup, Script, gpos, pair, words};

fn offset(out: &mut [u8], at: usize, value: usize) {
    out[at..at + 2].copy_from_slice(&u16::try_from(value).unwrap().to_be_bytes());
}

// Shared-offset layouts from the independent review probes, without expanding their aliases.
pub fn alias_gpos(n: u16) -> Vec<u8> {
    let mut out = words(&[1, 0, 10, 0, 0, 1]);
    out.extend(b"DFLT");
    out.extend(words(&[8, 4, 0, 0, u16::MAX, 1, 0]));
    let at = out.len();
    offset(&mut out, 6, at);
    out.extend(words(&[1]));
    out.extend(b"kern");
    out.extend(words(&[8, 0, n]));
    out.extend(words(&(0..n).collect::<Vec<_>>()));
    let at = out.len();
    offset(&mut out, 8, at);
    out.extend(words(&[n]));
    out.extend(words(&vec![2 + n * 2; usize::from(n)]));
    out.extend(words(&[2, 0, n]));
    out.extend(words(&vec![6 + n * 2; usize::from(n)]));
    out.extend(pair(1, -100, false, false));
    out
}

pub fn repeated_features(n: u16) -> Vec<u8> {
    let mut out = words(&[1, 0, 10, 22, 0, 1]);
    out.extend(b"DFLT");
    out.extend(words(&[8, 0, 0, 1]));
    out.extend(b"kern");
    out.extend(words(&[8, 0, n]));
    out.extend(words(&vec![0; usize::from(n)]));
    let at = out.len();
    offset(&mut out, 8, at);
    out.extend(words(&[1, 4, 2, 0, 1, 8]));
    out.extend(pair(1, -100, false, false));
    let at = out.len();
    offset(&mut out, 18, at - 18);
    out.extend(words(&[0, u16::MAX, n]));
    out.extend(words(&vec![0; usize::from(n)]));
    out
}

pub fn malformed_feature() -> Vec<u8> {
    let mut table = gpos(
        &[Script {
            tag: *b"DFLT",
            required: None,
            features: vec![1],
        }],
        &[
            Feature {
                tag: *b"aaaa",
                lookups: vec![],
            },
            Feature::kern(vec![0]),
        ],
        &[Lookup::pair(1, false)],
    );
    let list = usize::from(u16::from_be_bytes([table[6], table[7]]));
    offset(&mut table, list + 6, usize::from(u16::MAX));
    table
}

pub fn multiple_seconds() -> Vec<u8> {
    let mut out = words(&[1, 26, 4, 0, 1, 12, 3, 0]);
    out.extend((-30_i16).to_be_bytes());
    out.extend(words(&[1]));
    out.extend((-50_i16).to_be_bytes());
    out.extend(words(&[2]));
    out.extend((-100_i16).to_be_bytes());
    out.extend(words(&[1, 1, 1]));
    out
}

//! MD4 (RFC 1320). Unity derives the fileIDs of scripts compiled into DLLs from it.

type Round = (fn(u32, u32, u32) -> u32, u32, [u32; 4], fn(usize) -> usize);

const ROUNDS: [Round; 3] = [
    (|x, y, z| (x & y) | (!x & z), 0, [3, 7, 11, 19], |i| i),
    (
        |x, y, z| (x & y) | (x & z) | (y & z),
        0x5a82_7999,
        [3, 5, 9, 13],
        |i| (i % 4) * 4 + i / 4,
    ),
    (
        |x, y, z| x ^ y ^ z,
        0x6ed9_eba1,
        [3, 9, 11, 15],
        |i| [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15][i],
    ),
];

pub(crate) fn digest(message: &[u8]) -> [u8; 16] {
    let bits = u64::try_from(message.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bits.to_le_bytes());
    let mut state = [0x6745_2301_u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for block in padded.chunks_exact(64) {
        compress(&mut state, block);
    }
    let mut out = [0; 16];
    for (bytes, word) in out.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn compress(state: &mut [u32; 4], block: &[u8]) {
    let mut words = [0_u32; 16];
    for (word, bytes) in words.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    for (function, constant, shifts, order) in ROUNDS {
        // Each step updates one variable; rotating the names keeps the RFC's a, d, c, b order.
        for step in 0..16 {
            let value = a
                .wrapping_add(function(b, c, d))
                .wrapping_add(words[order(step)])
                .wrapping_add(constant)
                .rotate_left(shifts[step % 4]);
            (a, b, c, d) = (d, value, b, c);
        }
    }
    for (word, value) in state.iter_mut().zip([a, b, c, d]) {
        *word = word.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    fn hex(bytes: [u8; 16]) -> String {
        bytes.iter().fold(String::new(), |mut text, byte| {
            write!(text, "{byte:02x}").unwrap();
            text
        })
    }

    #[test]
    fn rfc_1320_test_suite() {
        let cases = [
            ("", "31d6cfe0d16ae931b73c59d7e0c089c0"),
            ("a", "bde52cb31de33e46245e05fbdbd6fb24"),
            ("abc", "a448017aaf21d8525fc10ae87aa6729d"),
            ("message digest", "d9130a8164549fe818874806e1c7014b"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "d79e1c308aa5bbcdeea8ed63df412da9",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "043f8582f241db351ce627e153e7f0e4",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "e33b4ddc9c38f2199c3e7b164fcc0536",
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(
                hex(super::digest(message.as_bytes())),
                expected,
                "{message:?}"
            );
        }
    }
}

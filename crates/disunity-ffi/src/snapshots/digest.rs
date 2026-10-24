// SHA-256 (FIPS 180-4). Kept local because the frozen workspace lock does not
// declare sha2 for this crate and this slice cannot modify that lock.
const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub(super) fn sha256(bytes: &[u8]) -> String {
    let mut state = INITIAL;
    let mut chunks = bytes.chunks_exact(64);
    for block in &mut chunks {
        compress(&mut state, block);
    }
    let rest = chunks.remainder();
    let mut tail = [0_u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let size = if rest.len() < 56 { 64 } else { 128 };
    tail[size - 8..size].copy_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for block in tail[..size].chunks_exact(64) {
        compress(&mut state, block);
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut words = [0_u32; 64];
    for (word, bytes) in words[..16].iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes(bytes.try_into().unwrap());
    }
    for i in 16..64 {
        let x = words[i - 15];
        let y = words[i - 2];
        let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        words[i] = words[i - 16]
            .wrapping_add(s0)
            .wrapping_add(words[i - 7])
            .wrapping_add(s1);
    }
    let mut work = *state;
    for (k, word) in K.iter().zip(words) {
        round(&mut work, k.wrapping_add(word));
    }
    for (out, value) in state.iter_mut().zip(work) {
        *out = out.wrapping_add(value);
    }
}
fn round(work: &mut [u32; 8], kw: u32) {
    let [a, b, c, d, e, f, g, h] = *work;
    let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
    let choice = (e & f) ^ (!e & g);
    let t1 = h.wrapping_add(s1).wrapping_add(choice).wrapping_add(kw);
    let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
    let majority = (a & b) ^ (a & c) ^ (b & c);
    *work = [
        t1.wrapping_add(s0.wrapping_add(majority)),
        a,
        b,
        c,
        d.wrapping_add(t1),
        e,
        f,
        g,
    ];
}

#[cfg(test)]
mod tests {
    use super::sha256;
    #[test]
    fn standard_sha256_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}

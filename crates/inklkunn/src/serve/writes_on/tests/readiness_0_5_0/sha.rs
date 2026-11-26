#![allow(clippy::expect_used, clippy::unwrap_used)]
//! SHA-256, written out for the proof: **the proof's checks of a bundle's checksums and of a history record's `sha256` must not be
//! the product's own code run again** (the product hashes with the `sha2` crate; a bug there would agree with itself). This is the
//! FIPS 180-4 function, checked against its published vectors the first time it is used. (A test may not index, so the schedule is
//! read with `at`.)

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

fn at(words: &[u32], i: usize) -> u32 {
    words.get(i).copied().unwrap_or(0)
}

/// The SHA-256 of `bytes`, as 64 lower-case hex digits.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h: Vec<u32> = vec![
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(bytes.len() as u64 * 8).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w: Vec<u32> = block
            .chunks_exact(4)
            .map(|c| c.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b)))
            .collect();
        for i in 16..64 {
            let (w15, w2) = (at(&w, i - 15), at(&w, i - 2));
            let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
            let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
            let next = at(&w, i - 16)
                .wrapping_add(s0)
                .wrapping_add(at(&w, i - 7))
                .wrapping_add(s1);
            w.push(next);
        }
        let mut v = [
            at(&h, 0),
            at(&h, 1),
            at(&h, 2),
            at(&h, 3),
            at(&h, 4),
            at(&h, 5),
            at(&h, 6),
            at(&h, 7),
        ];
        for (k, word) in K.iter().zip(&w) {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            v = [
                t1.wrapping_add(s0.wrapping_add(maj)),
                a,
                b,
                c,
                d.wrapping_add(t1),
                e,
                f,
                g,
            ];
        }
        h = h.iter().zip(v).map(|(x, y)| x.wrapping_add(y)).collect();
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// The function against the published vectors (FIPS 180-4's examples, and the empty string, which every `.keep` of a bundle is).
pub(super) fn known() -> bool {
    sha256_hex(b"") == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        && sha256_hex(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        && sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
            == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
}

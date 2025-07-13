//! Random number generation

use spin::Mutex;

use crate::time::system::Timestamp;

static GLOBAL_ENTROPY_POOL: Mutex<EntropyPool<44>> = Mutex::new(EntropyPool::new());

pub fn initialize_entropy_pool() {
    let mut pool = GLOBAL_ENTROPY_POOL.lock();
    // TODO: Fill the pool with some initial entropy
    for i in 0..pool.pool.len() {
        pool.pool[i] = (i as u8).wrapping_add(1);
    }
    pool.index = 0;
}

pub fn add_entropy(data: u32) {
    let mut pool = GLOBAL_ENTROPY_POOL.lock();
    for i in 0..8 {
        let index = pool.index;
        pool.pool[index] ^= (data >> (i * 4)) as u8;
        pool.index += 1;
        pool.index %= pool.pool.len();
    }
}

static GLOBAL_RNG: Mutex<ChaChaRng> = Mutex::new(ChaChaRng::new([0; 32], [0; 12]));

pub fn get_random_bytes(bytes: &mut [u8]) {
    let (last_reseed, generated_count) = {
        let rng = GLOBAL_RNG.lock();
        (rng.last_reseed, rng.generated_count)
    };
    let now = Timestamp::now();
    if now.as_u32() - last_reseed.as_u32() > 60 * 5 || generated_count > 1000 {
        // reseed the rng
        let mut seed_bytes: [u8; 44] = [0; 44];
        // TODO: implement BLAKE2 hashing before copying entropy bytes
        let seed_bytes = GLOBAL_ENTROPY_POOL.lock().pool;

        GLOBAL_RNG.lock().state = ChaChaRng::initial_state(
            seed_bytes[..32].try_into().unwrap(),
            seed_bytes[32..44].try_into().unwrap(),
        );
    }
    GLOBAL_RNG.lock().get_bytes(bytes);
}

struct EntropyPool<const N: usize> {
    pool: [u8; N],
    index: usize,
}

impl<const N: usize> EntropyPool<N> {
    pub const fn new() -> Self {
        Self {
            pool: [0; N],
            index: 0,
        }
    }
}

struct ChaChaRng {
    state: [u32; 16],

    /// Generate a keystream ahead of time to avoid running the algo every time
    /// bytes are requested
    keystream: [u8; 64],
    keystream_index: usize,

    last_reseed: Timestamp,
    generated_count: usize,
}

impl ChaChaRng {
    pub const fn initial_state(key: &[u8; 32], nonce: &[u8; 12]) -> [u32; 16] {
        // 4 constant words, 8 key words, 1 changing nonce word, 3 static nonce words
        [
            0x61707865, // "expa"
            0x3320646e, // "nd 3"
            0x79622d32, // "2-by"
            0x6b206574, // "te k"
            u32::from_le_bytes([key[0], key[1], key[2], key[3]]),
            u32::from_le_bytes([key[4], key[5], key[6], key[7]]),
            u32::from_le_bytes([key[8], key[9], key[10], key[11]]),
            u32::from_le_bytes([key[12], key[13], key[14], key[15]]),
            u32::from_le_bytes([key[16], key[17], key[18], key[19]]),
            u32::from_le_bytes([key[20], key[21], key[22], key[23]]),
            u32::from_le_bytes([key[24], key[25], key[26], key[27]]),
            u32::from_le_bytes([key[28], key[29], key[30], key[31]]),
            0,
            u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]),
            u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]),
            u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]),
        ]
    }

    pub const fn new(key: [u8; 32], nonce: [u8; 12]) -> Self {
        Self {
            state: Self::initial_state(&key, &nonce),
            keystream: [0; 64],
            keystream_index: 64,

            last_reseed: Timestamp(0),
            generated_count: 0,
        }
    }

    fn quarter_round(&mut self, a: usize, b: usize, c: usize, d: usize) {
        self.state[a] = self.state[a].wrapping_add(self.state[b]);
        self.state[d] = self.state[d] ^ self.state[a];
        self.state[d] = self.state[d].rotate_left(16);

        self.state[c] = self.state[c].wrapping_add(self.state[d]);
        self.state[b] = self.state[b] ^ self.state[c];
        self.state[b] = self.state[b].rotate_left(12);

        self.state[a] = self.state[a].wrapping_add(self.state[b]);
        self.state[d] = self.state[d] ^ self.state[a];
        self.state[d] = self.state[d].rotate_left(8);

        self.state[c] = self.state[c].wrapping_add(self.state[d]);
        self.state[b] = self.state[b] ^ self.state[c];
        self.state[b] = self.state[b].rotate_left(7);
    }

    pub fn generate_keystream(&mut self) {
        let mut working_state = self.state;
        for _ in 0..10 {
            self.quarter_round(0, 4, 8, 12);
            self.quarter_round(1, 5, 9, 13);
            self.quarter_round(2, 6, 10, 14);
            self.quarter_round(3, 7, 11, 15);

            self.quarter_round(0, 5, 10, 15);
            self.quarter_round(1, 6, 11, 12);
            self.quarter_round(2, 7, 8, 13);
            self.quarter_round(3, 4, 9, 14);
        }

        for i in 0..16 {
            working_state[i] = working_state[i].wrapping_add(self.state[i]);
        }

        for i in 0..16 {
            self.keystream[i * 4] = (working_state[i] & 0xFF) as u8;
            self.keystream[i * 4 + 1] = ((working_state[i] >> 8) & 0xFF) as u8;
            self.keystream[i * 4 + 2] = ((working_state[i] >> 16) & 0xFF) as u8;
            self.keystream[i * 4 + 3] = ((working_state[i] >> 24) & 0xFF) as u8;
        }

        self.state[12] = self.state[12].wrapping_add(1);

        self.keystream_index = 0;
    }

    pub fn get_bytes(&mut self, bytes: &mut [u8]) {
        let mut index = 0;
        while index < bytes.len() {
            if self.keystream_index >= self.keystream.len() {
                self.generate_keystream();
            }
            let remaining = bytes.len() - index;
            let to_copy = remaining.min(self.keystream.len() - self.keystream_index);
            bytes[index..index + to_copy].copy_from_slice(
                &self.keystream[self.keystream_index..self.keystream_index + to_copy],
            );
            index += to_copy;
            self.keystream_index += to_copy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChaChaRng;

    fn byte_slice_from_hex_string<'a>(hex: &'a str) -> impl Iterator<Item = u8> + 'a {
        hex.as_bytes().chunks(2).map(|pair| {
            let byte_str = core::str::from_utf8(pair).unwrap();
            u8::from_str_radix(byte_str, 16).unwrap()
        })
    }

    #[test_case]
    fn test_known_vectors() {
        {
            let key: [u8; 32] = [0; 32];
            let nonce: [u8; 12] = [0; 12];
            let mut rng = ChaChaRng::new(key, nonce);
            rng.generate_keystream();
            let mut expected = byte_slice_from_hex_string(
                "76b8e0ada0f13d90405d6ae55386bd28bdd219b8a08ded1aa836efcc8b770dc7da41597c5157488d7724e03fb8d84a376a43b8f41518a11cc387b669b2ee6586"
            );
            for i in 0..64 {
                assert_eq!(rng.keystream[i], expected.next().unwrap());
            }
        }

        {
            let key: [u8; 32] = [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 1,
            ];
            let nonce: [u8; 12] = [0; 12];
            let mut rng = ChaChaRng::new(key, nonce);
            rng.generate_keystream();
            let mut expected = byte_slice_from_hex_string(
               "4540f05a9f1fb296d7736e7b208e3c96eb4fe1834688d2604f450952ed432d41bbe2a0b6ea7566d2a5d1e7e20d42af2c53d792b1c43fea817e9ad275ae546963"
            );
            for i in 0..64 {
                assert_eq!(rng.keystream[i], expected.next().unwrap());
            }
        }

        {
            let key: [u8; 32] = [0; 32];
            let nonce: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
            let mut rng = ChaChaRng::new(key, nonce);
            rng.generate_keystream();
            let mut expected = byte_slice_from_hex_string(
                "de9cba7bf3d69ef5e786dc63973f653a0b49e015adbff7134fcb7df137821031e85a050278a7084527214f73efc7fa5b5277062eb7a0433e445f41e3",
            );
            for i in 0..60 {
                assert_eq!(rng.keystream[i], expected.next().unwrap());
            }
        }

        {
            let key: [u8; 32] = [0; 32];
            let nonce: [u8; 12] = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
            let mut rng = ChaChaRng::new(key, nonce);
            rng.generate_keystream();
            let mut expected = byte_slice_from_hex_string(
                "ef3fdfd6c61578fbf5cf35bd3dd33b8009631634d21e42ac33960bd138e50d32111e4caf237ee53ca8ad6426194a88545ddc497a0b466e7d6bbdb0041b2f586b"
            );
            for i in 0..64 {
                assert_eq!(rng.keystream[i], expected.next().unwrap());
            }
        }
    }
}

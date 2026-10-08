//! Small sound files built in memory, for tests (the `sound` unit tests and the game crate's
//! audio tests). Not used by the game itself.

/// A 16-bit PCM WAV.
pub fn wav(samples: &[i16], rate: u32, ch: u16) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&ch.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2 * u32::from(ch)).to_le_bytes());
    b.extend_from_slice(&(2 * ch).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// An IMA-ADPCM mono WAV: `blocks` blocks of 256 bytes (505 frames each) with pseudo-random
/// nibbles.
pub fn ima_adpcm_wav(blocks: usize) -> Vec<u8> {
    let (block_align, spb, rate) = (256u16, 505u16, 22_050u32);
    let data_len = (blocks * usize::from(block_align)) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(4 + 28 + 8 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&20u32.to_le_bytes());
    b.extend_from_slice(&0x11u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * u32::from(block_align) / u32::from(spb)).to_le_bytes());
    b.extend_from_slice(&block_align.to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&spb.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    let mut x = 0x1234_5678u32;
    for _ in 0..blocks {
        b.extend_from_slice(&[0, 0, 20, 0]);
        for _ in 0..usize::from(block_align) - 4 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            b.push((x >> 16) as u8);
        }
    }
    b
}

/// An MS-ADPCM WAV (`channels` channels, 256-byte blocks) with pseudo-random nibbles masked by
/// `nibble_mask`: `0x11` keeps them small; `0xFF` (large nibbles) overflows symphonia 0.5.5's
/// step size in builds with overflow checks.
pub fn ms_adpcm_wav(blocks: usize, channels: u16, nibble_mask: u8) -> Vec<u8> {
    let block_align = 256u16;
    let data_len = (blocks * usize::from(block_align)) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(4 + 58 + 8 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&50u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&22_050u32.to_le_bytes());
    b.extend_from_slice(&11_155u32.to_le_bytes());
    b.extend_from_slice(&block_align.to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    let spb = (block_align - 7 * channels) * 2 / channels + 2;
    b.extend_from_slice(&spb.to_le_bytes());
    b.extend_from_slice(&7u16.to_le_bytes());
    for (c1, c2) in [(256i16, 0i16), (512, -256), (0, 0), (192, 64), (240, 0), (460, -208), (392, -232)] {
        b.extend_from_slice(&c1.to_le_bytes());
        b.extend_from_slice(&c2.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    let mut x = 0x2468_ACE1u32;
    for _ in 0..blocks {
        let mut block = Vec::new();
        block.extend(std::iter::repeat_n(0u8, usize::from(channels)));
        for _ in 0..channels {
            block.extend_from_slice(&16i16.to_le_bytes());
        }
        block.extend(std::iter::repeat_n(0u8, 4 * usize::from(channels)));
        while block.len() < usize::from(block_align) {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            block.push(((x >> 16) as u8) & nibble_mask);
        }
        b.extend_from_slice(&block);
    }
    b
}

/// `n` silent 128 kbit/s 44.1 kHz stereo MPEG-1 Layer III frames (417/418 bytes), optionally
/// CRC-protected, with "Info" at byte `info_at` of the first frame, and frame `bad` given
/// all-ones side info (`big_values` > 288), which the decoder rejects.
pub fn mp3(n: usize, crc: bool, info_at: Option<usize>, bad: Option<usize>) -> Vec<u8> {
    let mut b = Vec::new();
    for i in 0..n {
        let pad = i % 3 == 0;
        let h: u32 = if crc { 0xFFFA_9000 } else { 0xFFFB_9000 } | (u32::from(pad) << 9);
        let mut f = vec![0u8; 417 + usize::from(pad)];
        f[..4].copy_from_slice(&h.to_be_bytes());
        if let (0, Some(o)) = (i, info_at) {
            f[o..o + 4].copy_from_slice(b"Info");
        }
        if bad == Some(i) {
            f[4..36].fill(0xFF);
        }
        b.extend_from_slice(&f);
    }
    b
}

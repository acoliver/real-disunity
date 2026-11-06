/// Four mono PCM16 frames at 4 Hz, lasting exactly one second.
pub(in crate::retained) fn clip() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44_u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(4_u32.to_le_bytes());
    bytes.extend(8_u32.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8_u32.to_le_bytes());
    bytes.extend([0; 8]);
    bytes
}

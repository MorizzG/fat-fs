pub fn load_u16_le(bytes: &[u8]) -> u16 {
    assert_eq!(bytes.len(), 2);

    u16::from_le_bytes(bytes.try_into().unwrap())
}

pub fn load_u32_le(bytes: &[u8]) -> u32 {
    assert_eq!(bytes.len(), 4);

    u32::from_le_bytes(bytes.try_into().unwrap())
}

pub fn load_u16_le(bytes: &[u8]) -> u16 {
    assert_eq!(bytes.len(), 2);

    u16::from_le_bytes(bytes.try_into().unwrap())
}
pub fn load_u32_le(bytes: &[u8]) -> u32 {
    assert_eq!(bytes.len(), 4);

    u32::from_le_bytes(bytes.try_into().unwrap())
}

/// replace the value at x with f(x)
///
/// SAFETY:
/// should be safe, I guess? MIRI didn't complain about it
pub fn replace<T>(x: &mut T, f: impl FnOnce(T) -> T) {
    unsafe {
        let x_ptr = x as *mut T;

        let old_x = std::ptr::read(x_ptr);

        let new_x = f(old_x);

        std::ptr::write(x_ptr, new_x);
    }
}

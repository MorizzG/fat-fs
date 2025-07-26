use crate::utils::load_u32_le;

pub struct FsInfo {
    free_count: u32,
    next_free: u32,
}

impl FsInfo {
    pub fn load(bytes: &[u8]) -> anyhow::Result<FsInfo> {
        let lead_sig = load_u32_le(&bytes[..4]);

        anyhow::ensure!(
            lead_sig == 0x41615252,
            "invalid lead signature: 0x{:#08X} instead of 0x41615252",
            lead_sig
        );

        let struct_sig = load_u32_le(&bytes[484..][..4]);

        anyhow::ensure!(
            struct_sig == 0x61417272,
            "invalid structural signature: 0x{:#08X} instead of 0x61417272",
            struct_sig
        );

        let trail_sig = load_u32_le(&bytes[508..][..4]);

        anyhow::ensure!(
            trail_sig == 0xAA550000,
            "invalid trailing signature: 0x{:#08X} instead of 0xAA550000",
            trail_sig
        );

        let free_count = load_u32_le(&bytes[488..][..4]);
        let next_free = load_u32_le(&bytes[492..][..4]);

        Ok(FsInfo {
            free_count,
            next_free,
        })
    }
}

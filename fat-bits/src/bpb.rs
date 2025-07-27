use std::fmt::Display;

use crate::FatType;
use crate::utils::{load_u16_le, load_u32_le};

#[derive(Debug)]
pub enum ExtBpb {
    ExtBpb16(ExtBpb16),
    ExtBpb32(ExtBpb32),
}

impl Display for ExtBpb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtBpb::ExtBpb16(ext_bpb16) => write!(f, "{}", ext_bpb16),
            ExtBpb::ExtBpb32(ext_bpb32) => write!(f, "{}", ext_bpb32),
        }
    }
}

#[derive(Debug)]
pub struct Bpb {
    fat_type: FatType,

    // jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sector_count: u16,
    num_fats: u8,
    root_entry_count: u16,
    total_sectors_16: u16,
    media: u8,
    fat_size_16: u16,
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,

    ext_bpb: ExtBpb,
}

impl Display for Bpb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Bpb {{")?;

        match self.fat_type {
            FatType::Fat12 => writeln!(f, "    FAT12")?,
            FatType::Fat16 => writeln!(f, "    FAT16")?,
            FatType::Fat32 => writeln!(f, "    FAT32")?,
        }

        writeln!(f, "")?;

        // writeln!(
        //     f,
        //     "    jmp_boot: [{:#X}, {:#X}, {:#X}]",
        //     self.jmp_boot[0], self.jmp_boot[1], self.jmp_boot[2]
        // )?;

        writeln!(f, "    oem name: \"{}\"", self.oem_name_str().unwrap_or(""))?;
        writeln!(f, "    bytes per sector: {}", self.bytes_per_sector())?;
        writeln!(f, "    sectors per cluster: {}", self.sectors_per_cluster())?;
        writeln!(f, "    reserved sector count: {}", self.reserved_sector_count())?;
        writeln!(f, "    num_fats: {}", self.num_fats())?;
        writeln!(f, "    root entry count: {}", self.root_entry_count())?;
        writeln!(f, "    total sectors: {}", self.total_sectors_16())?;
        writeln!(f, "    media: {:#X}", self.media())?;
        writeln!(f, "    fat_size_16: {}", self.fat_size_16())?;
        writeln!(f, "    sectors per track: {}", self.sectors_per_track())?;
        writeln!(f, "    num_heads: {}", self.num_heads())?;
        writeln!(f, "    hidden_sectors: {}", self.hidden_sectors())?;
        writeln!(f, "    total sectors 32: {}", self.total_sectors_32())?;

        writeln!(f, "")?;

        let ext_bpb_str = format!("{}", self.ext_bpb);

        for line in ext_bpb_str.lines() {
            writeln!(f, "    {}", line)?;
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl Bpb {
    pub fn load(bytes: &[u8]) -> anyhow::Result<Bpb> {
        anyhow::ensure!(bytes.len() >= 512, "invalid BPB of len {}", bytes.len());

        // let jmp_boot = bytes[..3].try_into().unwrap();

        let oem_name = bytes[3..][..8].try_into().unwrap();
        let bytes_per_sector = load_u16_le(&bytes[11..][..2]);

        if !&[512, 1024, 2048, 4096].contains(&bytes_per_sector) {
            anyhow::bail!("invalid bytes per sector: {}", bytes_per_sector);
        }

        let sectors_per_cluster = bytes[13];

        if !&[1, 2, 4, 8, 16, 32, 64, 128].contains(&sectors_per_cluster) {
            anyhow::bail!("invalid sectors per cluster: {}", sectors_per_cluster);
        }

        let reserved_sector_count = load_u16_le(&bytes[14..][..2]);

        anyhow::ensure!(reserved_sector_count != 0, "reserved sector count can't be zero");

        let num_fats = bytes[16];
        let root_entry_count = load_u16_le(&bytes[17..][..2]);
        let total_sectors_16 = load_u16_le(&bytes[19..][..2]);

        let media = bytes[21];

        if !&[0xF0, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF].contains(&media) {
            anyhow::bail!("invalid media: {}", media);
        }

        let fat_size_16 = load_u16_le(&bytes[22..][..2]);
        let sectors_per_track = load_u16_le(&bytes[24..][..2]);
        let num_heads = load_u16_le(&bytes[26..][..2]);
        let hidden_sectors = load_u32_le(&bytes[28..][..4]);
        let total_sectors_32 = load_u32_le(&bytes[32..][..4]);

        let (fat_type, ext_bpb) = if fat_size_16 == 0 {
            // FAT32?

            anyhow::ensure!(
                total_sectors_16 == 0,
                "fat_size_16 is 0, but total sectors is {} instead of 0",
                total_sectors_16
            );

            let ext_bpb = ExtBpb32::load(bytes)?;
            (FatType::Fat32, ExtBpb::ExtBpb32(ext_bpb))
        } else {
            // FAT16?

            let ext_bpb = ExtBpb16::load(bytes)?;

            (FatType::Fat16, ExtBpb::ExtBpb16(ext_bpb))
        };

        let mut bpb = Bpb {
            fat_type,
            // jmp_boot,
            oem_name,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sector_count,
            num_fats,
            root_entry_count,
            total_sectors_16,
            media,
            fat_size_16,
            sectors_per_track,
            num_heads,
            hidden_sectors,
            total_sectors_32,
            ext_bpb,
        };

        let count_of_clusters = bpb.count_of_clusters();

        if count_of_clusters < 4085 {
            anyhow::ensure!(
                bpb.fat_type == FatType::Fat16,
                "{:?} should actually be FAT12",
                bpb.fat_type
            );

            // actually FAT12 instead of FAT16
            bpb.fat_type = FatType::Fat12;
        } else if count_of_clusters < 65525 {
            anyhow::ensure!(
                bpb.fat_type == FatType::Fat16,
                "{:?} should actually be FAT16",
                bpb.fat_type
            );
        } else {
            anyhow::ensure!(
                bpb.fat_type == FatType::Fat32,
                "{:?} should actually be FAT32",
                bpb.fat_type
            );
        }

        Ok(bpb)
    }

    /// number of sectors usable for data
    pub fn num_data_sectors(&self) -> u32 {
        let data_sectors = self.total_sectors()
            - (self.reserved_sector_count() as u32
                + (self.num_fats() as u32 * self.fat_size())
                + self.root_dir_sectors());

        data_sectors
    }

    /// total number of clusters on this volume
    pub fn num_clusters(&self) -> u32 {
        self.total_sectors() / self.sectors_per_cluster() as u32
    }

    /// number of bytes per cluster
    pub fn bytes_per_cluster(&self) -> usize {
        self.sectors_per_cluster() as usize * self.bytes_per_sector() as usize
    }

    /// count of *data* clusters
    pub fn count_of_clusters(&self) -> u32 {
        self.num_data_sectors() / self.sectors_per_cluster as u32
    }

    /// convert a given sector to an byte offset
    fn sector_to_offset(&self, sector: u32) -> u64 {
        sector as u64 * self.bytes_per_sector() as u64
    }

    /// byte offset of the first FAT
    pub fn fat_offset(&self) -> u64 {
        self.sector_to_offset(self.reserved_sector_count() as u32)
    }

    /// FAT size in bytes
    pub fn fat_len_bytes(&self) -> usize {
        self.bytes_per_sector() as usize * self.fat_size() as usize
    }

    /// byte offset of the root directory; None for FAT32
    pub fn root_directory_offset(&self) -> Option<u64> {
        if self.fat_type() == FatType::Fat32 {
            return None;
        }
        Some(self.fat_offset() + self.sector_to_offset(self.num_fats() as u32 * self.fat_size()))
    }

    /// number of sectors for root dir (only FAT12 and FAT16)
    pub fn root_dir_sectors(&self) -> u32 {
        (32 * self.root_entry_count() as u32).div_ceil(self.bytes_per_sector() as u32)
    }

    /// byte size of the root directory
    pub fn root_dir_len_bytes(&self) -> usize {
        self.root_dir_sectors() as usize * self.bytes_per_sector() as usize
    }

    /// first data sector
    pub fn first_data_sector(&self) -> u32 {
        self.reserved_sector_count() as u32
            + (self.num_fats() as u32 * self.fat_size())
            + self.root_dir_sectors() as u32
    }

    pub fn data_offset(&self) -> u64 {
        // if let Some(root_dir_offset) = self.root_directory_offset() {
        //     // has root directory (FAT12 or FAT16)
        //     return root_dir_offset + self.sector_to_offset(self.root_entry_count() as u32);
        // }

        // self.fat_offset() + self.sector_to_offset(self.num_fats() as u32 * self.fat_size())

        self.sector_to_offset(self.first_data_sector())
    }

    /// byte size of the data section
    pub fn data_len_bytes(&self) -> usize {
        self.num_data_sectors() as usize * self.bytes_per_sector() as usize
    }

    /// FAT type (FAT12, FAT16, or FAT32)
    pub fn fat_type(&self) -> FatType {
        self.fat_type
    }

    /// number of sectors per FAT
    pub fn fat_size(&self) -> u32 {
        match &self.ext_bpb {
            ExtBpb::ExtBpb16(_ext_bpb16) => self.fat_size_16() as u32,
            ExtBpb::ExtBpb32(ext_bpb32) => ext_bpb32.fat_size_32(),
        }
    }

    /// get root cluster for FAT32
    pub fn root_cluster(&self) -> Option<u32> {
        if let ExtBpb::ExtBpb32(ext_bpb32) = &self.ext_bpb {
            Some(ext_bpb32.root_cluster())
        } else {
            None
        }
    }

    /// total number of sectors in this device
    ///
    /// uses total_sectors_16 or total_sectors_32
    pub fn total_sectors(&self) -> u32 {
        // match &self.ext_bpb {
        //     ExtBpb::ExtBpb16(_) => self.total_sectors_16() as u32,
        //     ExtBpb::ExtBpb32(_) => self.total_sectors_32(),
        // }

        match self.total_sectors_16() {
            0 => self.total_sectors_32(),
            n => n as u32,
        }
    }

    pub fn oem_name(&self) -> &[u8] {
        &self.oem_name
    }

    pub fn oem_name_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.oem_name()).ok()
    }

    pub fn bytes_per_sector(&self) -> u16 {
        self.bytes_per_sector
    }

    pub fn sectors_per_cluster(&self) -> u8 {
        self.sectors_per_cluster
    }

    pub fn reserved_sector_count(&self) -> u16 {
        self.reserved_sector_count
    }

    pub fn num_fats(&self) -> u8 {
        self.num_fats
    }

    // number of 32 byte dir entries in the root directory
    pub fn root_entry_count(&self) -> u16 {
        self.root_entry_count
    }

    pub fn total_sectors_16(&self) -> u16 {
        self.total_sectors_16
    }

    pub fn media(&self) -> u8 {
        self.media
    }

    pub fn fat_size_16(&self) -> u16 {
        self.fat_size_16
    }

    pub fn sectors_per_track(&self) -> u16 {
        self.sectors_per_track
    }

    pub fn num_heads(&self) -> u16 {
        self.num_heads
    }

    pub fn hidden_sectors(&self) -> u32 {
        self.hidden_sectors
    }

    pub fn total_sectors_32(&self) -> u32 {
        self.total_sectors_32
    }
}

#[derive(Debug)]
pub struct ExtBpb16 {
    drive_number: u8,
    boot_sig: u8,
    volume_serial_number: u32,
    volume_label: [u8; 11],
    file_sys_type: [u8; 8],
}

impl Display for ExtBpb16 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ExtBpb16 {{")?;

        writeln!(f, "    drive number: {}", self.drive_number())?;
        writeln!(f, "    boot_sig: {:#x}", self.boot_sig())?;
        writeln!(f, "    volume serial number: {}", self.volume_serial_number())?;
        writeln!(f, "    volume label: {}", self.volume_label_str().unwrap_or(""))?;
        writeln!(f, "    file sys type: {}", self.file_sys_type_str())?;

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl ExtBpb16 {
    pub fn load(bytes: &[u8]) -> anyhow::Result<ExtBpb16> {
        let drive_number = bytes[36];

        if !&[0x80, 0x00].contains(&drive_number) {
            anyhow::bail!("invalid drive number: {}", drive_number);
        }

        let boot_sig = bytes[38];
        let volume_serial_number = load_u32_le(&bytes[39..][..4]);
        let volume_label = bytes[43..][..11].try_into().unwrap();

        if volume_serial_number != 0 || volume_label != [0; 11] {
            anyhow::ensure!(
                boot_sig == 0x29,
                "volume serial number and volume label are not both empty, but boot sig is {:#x}
        instead of 0x29",
                boot_sig
            );
        }

        let file_sys_type: [u8; 8] = bytes[54..][..8].try_into().unwrap();

        let Some(s) = std::str::from_utf8(&file_sys_type).ok() else {
            anyhow::bail!("invalid file sys type: {:X?}", file_sys_type);
        };

        if !&["FAT12   ", "FAT16   ", "FAT     "].contains(&s) {
            anyhow::bail!("invalid file sys type: {}", s);
        }

        let signature_word = &bytes[510..512];

        anyhow::ensure!(
            signature_word == &[0x55, 0xAA],
            "invalid signature word: [{:#X}, {:#X}] instead of [0x55, 0xAA]",
            bytes[510],
            bytes[511]
        );

        Ok(ExtBpb16 {
            drive_number,
            boot_sig,
            volume_serial_number,
            volume_label,
            file_sys_type,
        })
    }

    pub fn drive_number(&self) -> u8 {
        self.drive_number
    }

    pub fn boot_sig(&self) -> u8 {
        self.boot_sig
    }

    pub fn volume_serial_number(&self) -> u32 {
        self.volume_serial_number
    }

    pub fn volume_label(&self) -> &[u8] {
        &self.volume_label
    }

    pub fn volume_label_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.volume_label).ok()
    }

    pub fn file_sys_type(&self) -> &[u8] {
        &self.file_sys_type
    }

    pub fn file_sys_type_str(&self) -> &str {
        std::str::from_utf8(&self.file_sys_type).unwrap()
    }
}

#[derive(Debug)]
pub struct ExtBpb32 {
    fat_size_32: u32,
    ext_flags: u16,
    root_cluster: u32,
    fs_info: u16,
    bk_boot_sector: u16,
    drive_number: u8,
    boot_sig: u8,
    volume_serial_number: u32,
    volume_label: [u8; 11],
}

impl Display for ExtBpb32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ExtBpb32 {{")?;

        writeln!(f, "    fat_size_32: {}", self.fat_size_32)?;
        writeln!(f, "    ext_flags: {}", self.ext_flags)?;
        writeln!(f, "    root_cluster: {}", self.root_cluster)?;
        writeln!(f, "    fs_info: {}", self.fs_info)?;
        writeln!(f, "    bk_boot_sector: {}", self.bk_boot_sector)?;
        writeln!(f, "    drive_number: {}", self.drive_number)?;
        writeln!(f, "    boot_sig: {:#X}", self.boot_sig)?;
        writeln!(f, "    volume serial number: {}", self.volume_serial_number)?;
        writeln!(f, "    volume label: {}", self.volume_label_str().unwrap_or(""))?;

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl ExtBpb32 {
    pub fn load(bytes: &[u8]) -> anyhow::Result<ExtBpb32> {
        let fat_size_32 = load_u32_le(&bytes[36..][..4]);

        anyhow::ensure!(fat_size_32 != 0, "fat_size_32 is zero");

        let ext_flags = load_u16_le(&bytes[40..][..2]);

        let fs_ver = load_u16_le(&bytes[42..][..2]);
        anyhow::ensure!(fs_ver == 0x0, "invalid FSVer: {}", fs_ver);

        let root_cluster = load_u32_le(&bytes[44..][..4]);
        let fs_info = load_u16_le(&bytes[48..][..2]);

        let bk_boot_sector = load_u16_le(&bytes[50..][..2]);

        anyhow::ensure!(
            &[0, 6].contains(&bk_boot_sector),
            "invalid BkBootSector: {}",
            bk_boot_sector
        );

        let reserved = &bytes[52..][..12];
        anyhow::ensure!(reserved == &[0; 12], "reserved is not zeroed");

        let drive_number = bytes[64];

        let reserved1 = bytes[65];
        anyhow::ensure!(reserved1 == 0, "reserved1 is not zeroed");

        let boot_sig = bytes[66];
        let volume_serial_number = load_u32_le(&bytes[67..][..4]);
        let volume_label = bytes[71..][..11].try_into().unwrap();

        if volume_serial_number != 0 || &volume_label != &[0; 11] {
            anyhow::ensure!(
                boot_sig == 0x29,
                "VollID or VolLab is set, but BootSig is {} instead of 0x29",
                boot_sig
            );
        }

        let file_sys_type = &bytes[82..][..8];
        anyhow::ensure!(
            std::str::from_utf8(file_sys_type) == Ok("FAT32   "),
            "invalid file sys type"
        );

        let signature_word = &bytes[510..][..2];

        anyhow::ensure!(
            signature_word == &[0x55, 0xAA],
            "invalid signature word [{:#X}, {:#X}] instead of [0x55, 0xAA]",
            signature_word[0],
            signature_word[1]
        );

        Ok(ExtBpb32 {
            fat_size_32,
            ext_flags,
            root_cluster,
            fs_info,
            bk_boot_sector,
            drive_number,
            boot_sig,
            volume_serial_number,
            volume_label,
        })
    }

    pub fn fat_size_32(&self) -> u32 {
        self.fat_size_32
    }

    pub fn ext_flags(&self) -> u16 {
        self.ext_flags
    }

    pub fn root_cluster(&self) -> u32 {
        self.root_cluster
    }

    pub fn fs_info(&self) -> u16 {
        self.fs_info
    }

    pub fn bk_boot_sector(&self) -> u16 {
        self.bk_boot_sector
    }

    pub fn drive_number(&self) -> u8 {
        self.drive_number
    }

    pub fn boot_sig(&self) -> u8 {
        self.boot_sig
    }

    pub fn volume_serial_number(&self) -> u32 {
        self.volume_serial_number
    }

    pub fn volume_label(&self) -> &[u8] {
        &self.volume_label
    }

    pub fn volume_label_str(&self) -> Option<&str> {
        std::str::from_utf8(self.volume_label()).ok()
    }
}

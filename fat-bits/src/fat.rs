use std::fmt::Display;
use std::io::Write as _;
use std::mem::MaybeUninit;
use std::ops::RangeInclusive;

use enum_dispatch::enum_dispatch;

use crate::FatType;
use crate::subslice::SubSliceMut;

#[derive(Debug, thiserror::Error)]
pub enum FatError {
    #[error("can't get next cluster of free cluster")]
    FreeCluster,
    #[error("cluster {0} is reserved")]
    ReservedCluster(u32),
    #[error("cluster is defective")]
    DefectiveCluster,
    #[error("invalid next cluster 0x{0:0X}")]
    InvalidEntry(u32),
}

#[enum_dispatch]
pub trait FatOps {
    // get the next cluster
    // assumes the cluster is valid, i.e. allocated
    fn get_entry(&self, cluster: u32) -> u32;
    fn set_entry(&mut self, cluster: u32, entry: u32);

    fn valid_clusters(&self) -> RangeInclusive<u32>;
    fn reserved_clusters(&self) -> RangeInclusive<u32>;
    fn defective_cluster(&self) -> u32;
    fn reserved_eof_clusters(&self) -> RangeInclusive<u32>;
    fn eof_cluster(&self) -> u32;

    fn count_free_clusters(&self) -> usize {
        self.valid_clusters()
            .map(|cluster| self.get_entry(cluster))
            .filter(|&entry| entry == 0)
            .count()
    }

    fn write_to_disk(&self, sub_slice: SubSliceMut) -> std::io::Result<()>;
}

#[enum_dispatch(FatOps)]
pub enum Fat {
    Fat12(Fat12),
    Fat16(Fat16),
    Fat32(Fat32),
}

impl Display for Fat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Fat::Fat12(fat12) => write!(f, "{}", fat12),
            Fat::Fat16(fat16) => write!(f, "{}", fat16),
            Fat::Fat32(fat32) => write!(f, "{}", fat32),
        }
    }
}

impl Fat {
    pub fn new(fat_type: FatType, bytes: &[u8], max: u32) -> Fat {
        match fat_type {
            FatType::Fat12 => Fat::Fat12(Fat12::new(bytes, max)),
            FatType::Fat16 => Fat::Fat16(Fat16::new(bytes, max)),
            FatType::Fat32 => Fat::Fat32(Fat32::new(bytes, max)),
        }
    }

    pub fn get_next_cluster(&self, cluster: u32) -> Result<Option<u32>, FatError> {
        if cluster == 0x000 {
            // can't get next cluster for free cluster
            return Err(FatError::FreeCluster);
        }

        if self.reserved_clusters().contains(&cluster) {
            // can't get next cluster for reserved cluster
            return Err(FatError::ReservedCluster(cluster));
        }

        // defective cluster
        if cluster == self.defective_cluster() {
            // can't get next cluster for defective cluster
            return Err(FatError::DefectiveCluster);
        }

        if self.reserved_eof_clusters().contains(&cluster) {
            // Reserved and should not be used. May be interpreted as an allocated cluster and the
            // final cluster in the file (indicating end-of-file condition).
            //
            // can't get next entry for reserved cluster

            // return Ok(None);
            return Err(FatError::ReservedCluster(cluster));
        }

        let entry = self.get_entry(cluster);

        // interpret second reserved block as EOF here
        if entry == self.eof_cluster() || self.reserved_eof_clusters().contains(&entry) {
            return Ok(None);
        }

        // entry should be in the valid cluster range here; otherwise something went wrong
        if !self.valid_clusters().contains(&entry) {
            return Err(FatError::InvalidEntry(entry));
        }

        Ok(Some(entry))
    }
}

pub struct Fat12 {
    max: u32,

    next_sectors: Box<[u16]>,
}

impl Display for Fat12 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Fat 12 {{")?;

        for (i, &x) in self.next_sectors.iter().enumerate() {
            if x != 0 {
                writeln!(f, "    0x{:03X} -> 0x{:03X}", i, x)?;
            }
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl Fat12 {
    pub fn new(bytes: &[u8], max: u32) -> Fat12 {
        // for FAT12 max is always less than 4085
        assert!(max <= 4085);

        let mut next_sectors: Box<[MaybeUninit<u16>]> = Box::new_uninit_slice(max as usize + 1);

        for elem in next_sectors.iter_mut() {
            elem.write(0);
        }

        let mut next_sectors = unsafe { next_sectors.assume_init() };

        // assume bytes.len() is multiple of 3
        // TODO: fix later
        assert_eq!(bytes.len() % 3, 0);

        let (chunks, rem) = bytes.as_chunks::<3>();

        assert_eq!(rem.len(), 0);

        // TODO: correctly handle cases where max is larger than #FAT entries and v.v.
        for (idx, triple) in chunks.iter().take(max as usize / 2).enumerate() {
            // first (even) entry gets truncated
            let first = u16::from_le_bytes(triple[..2].try_into().unwrap()) & 0xFFF;
            // second (odd) entry gets shifted
            let second = u16::from_le_bytes(triple[1..].try_into().unwrap()) >> 4;

            assert!(idx + 1 < next_sectors.len());

            next_sectors[2 * idx] = first;
            next_sectors[2 * idx + 1] = second;
        }

        if max % 2 == 1 {
            // odd max: need to fix last cluster
            let idx = max as usize / 2 + 1;

            let triple = chunks[idx];

            let first = u16::from_le_bytes(triple[..2].try_into().unwrap()) & 0xFFF;

            next_sectors[2 * idx] = first;
        }

        Fat12 { max, next_sectors }
    }
}

impl FatOps for Fat12 {
    fn get_entry(&self, cluster: u32) -> u32 {
        let cluster = cluster as usize;
        assert!(cluster < self.next_sectors.len());

        self.next_sectors[cluster] as u32
    }

    fn set_entry(&mut self, cluster: u32, entry: u32) {
        self.next_sectors[cluster as usize] = entry as u16;
    }

    fn valid_clusters(&self) -> RangeInclusive<u32> {
        2..=self.max
    }

    fn reserved_clusters(&self) -> RangeInclusive<u32> {
        (self.max as u32 + 1)..=0xFF6
    }

    fn defective_cluster(&self) -> u32 {
        0xFF7
    }

    fn reserved_eof_clusters(&self) -> RangeInclusive<u32> {
        0xFF8..=0xFFE
    }

    fn eof_cluster(&self) -> u32 {
        0xFFF
    }

    fn write_to_disk(&self, mut sub_slice: SubSliceMut) -> std::io::Result<()> {
        // TODO: currently assumed FAT has even number of entries

        assert_eq!(3 * sub_slice.len(), self.next_sectors.len());

        let mut iter = self.next_sectors.chunks_exact(3);

        let mut buf: [u8; 3];

        for chunk in &mut iter {
            // first (even) entry gets truncated
            // let first = u16::from_le_bytes(triple[..2].try_into().unwrap()) & 0xFFF;
            // second (odd) entry gets shifted
            // let second = u16::from_le_bytes(triple[1..].try_into().unwrap()) >> 4;

            // assert!(idx + 1 < next_sectors.len());

            // next_sectors[2 * idx] = first;
            // next_sectors[2 * idx + 1] = second;

            // sub_slice.write_all(&entry.to_le_bytes())?;

            let first = chunk[0];
            let second = chunk[1];

            buf = [0; 3];

            // buf[..2] |= &first.to_le_bytes();
            buf[0] = first.to_le_bytes()[0];
            buf[1] = first.to_le_bytes()[1] | (second << 4).to_le_bytes()[0];
            buf[2] = (second << 4).to_le_bytes()[1];
            sub_slice.write_all(&buf)?;
        }

        assert_eq!(iter.remainder().len(), 0);

        Ok(())
    }
}

pub struct Fat16 {
    max: u32,

    next_sectors: Box<[u16]>,
}

impl Display for Fat16 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Fat 16 {{")?;

        for (i, &x) in self.next_sectors.iter().enumerate() {
            if x != 0 {
                writeln!(f, "    0x{:03X} -> 0x{:03X}", i, x)?;
            }
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl Fat16 {
    pub fn new(bytes: &[u8], max: u32) -> Fat16 {
        // for FAT12 max is always less than 4085
        assert!(4085 < max && max <= 65525);

        let mut next_sectors: Box<[MaybeUninit<u16>]> = Box::new_uninit_slice(max as usize + 1);

        for elem in next_sectors.iter_mut() {
            elem.write(0);
        }

        let mut next_sectors = unsafe { next_sectors.assume_init() };

        // assume bytes.len() is multiple of 2
        // TODO: fix later
        assert_eq!(bytes.len() % 2, 0);

        let (chunks, rem) = bytes.as_chunks::<2>();

        assert_eq!(rem.len(), 0);

        // TODO: correctly handle cases where max is larger than #FAT entries and v.v.
        for (idx, chunk) in chunks.iter().take(max as usize / 2).enumerate() {
            // first (even) entry gets truncated
            let entry = u16::from_le_bytes(chunk[..2].try_into().unwrap());

            next_sectors[idx] = entry;
        }

        Fat16 { max, next_sectors }
    }
}

impl FatOps for Fat16 {
    fn get_entry(&self, cluster: u32) -> u32 {
        let cluster = cluster as usize;
        assert!(cluster < self.next_sectors.len());

        self.next_sectors[cluster] as u32
    }

    fn set_entry(&mut self, cluster: u32, entry: u32) {
        self.next_sectors[cluster as usize] = entry as u16;
    }

    fn valid_clusters(&self) -> RangeInclusive<u32> {
        2..=self.max
    }

    fn reserved_clusters(&self) -> RangeInclusive<u32> {
        (self.max as u32 + 1)..=0xFFF6
    }

    fn defective_cluster(&self) -> u32 {
        0xFFF7
    }

    fn reserved_eof_clusters(&self) -> RangeInclusive<u32> {
        0xFFF8..=0xFFFE
    }

    fn eof_cluster(&self) -> u32 {
        0xFFFF
    }

    fn write_to_disk(&self, mut sub_slice: SubSliceMut) -> std::io::Result<()> {
        assert_eq!(2 * sub_slice.len(), self.next_sectors.len());

        for &entry in self.next_sectors.iter() {
            sub_slice.write_all(&entry.to_le_bytes())?;
        }

        Ok(())
    }
}

pub struct Fat32 {
    max: u32,

    next_sectors: Box<[u32]>,
}

impl Display for Fat32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Fat 32 {{")?;

        for (i, &x) in self.next_sectors.iter().enumerate() {
            if x != 0 {
                writeln!(f, "    0x{:03X} -> 0x{:03X}", i, x)?;
            }
        }

        writeln!(f, "}}")?;

        Ok(())
    }
}

impl Fat32 {
    pub fn new(bytes: &[u8], max: u32) -> Fat32 {
        // for FAT12 max is always less than 4085
        assert!(65525 < max);

        let mut next_sectors: Box<[MaybeUninit<u32>]> = Box::new_uninit_slice(max as usize + 1);

        for elem in next_sectors.iter_mut() {
            elem.write(0);
        }

        let mut next_sectors = unsafe { next_sectors.assume_init() };

        // assume bytes.len() is multiple of 4
        // TODO: fix later
        assert_eq!(bytes.len() % 4, 0);

        let (chunks, rem) = bytes.as_chunks::<4>();

        assert_eq!(rem.len(), 0);

        // TODO: correctly handle cases where max is larger than #FAT entries and v.v.
        for (idx, chunk) in chunks.iter().take(max as usize / 2).enumerate() {
            // first (even) entry gets truncated
            let entry = u32::from_le_bytes(chunk[..4].try_into().unwrap());

            next_sectors[idx] = entry;
        }

        Fat32 { max, next_sectors }
    }
}

impl FatOps for Fat32 {
    fn get_entry(&self, cluster: u32) -> u32 {
        let cluster = cluster as usize;
        assert!(cluster < self.next_sectors.len());

        self.next_sectors[cluster] as u32
    }

    fn set_entry(&mut self, cluster: u32, entry: u32) {
        self.next_sectors[cluster as usize] = entry;
    }

    fn valid_clusters(&self) -> RangeInclusive<u32> {
        2..=self.max
    }

    fn reserved_clusters(&self) -> RangeInclusive<u32> {
        (self.max + 1)..=0xFFFFFFF6
    }

    fn defective_cluster(&self) -> u32 {
        0xFFFFFFF7
    }

    fn reserved_eof_clusters(&self) -> RangeInclusive<u32> {
        0xFFFFFFF8..=0xFFFFFFFE
    }

    fn eof_cluster(&self) -> u32 {
        0xFFFFFFFF
    }

    fn write_to_disk(&self, mut sub_slice: SubSliceMut) -> std::io::Result<()> {
        assert_eq!(4 * sub_slice.len(), self.next_sectors.len());

        for &entry in self.next_sectors.iter() {
            sub_slice.write_all(&entry.to_le_bytes())?;
        }

        Ok(())
    }
}

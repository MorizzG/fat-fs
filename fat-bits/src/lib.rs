use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom, Write};
use std::rc::Rc;

use crate::dir::DirIter;
use crate::fat::{FatError, Fatty};
use crate::subslice::{SubSlice, SubSliceMut};

pub mod bpb;
mod datetime;
pub mod dir;
pub mod fat;
pub mod fs_info;
mod iter;
mod subslice;
mod utils;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

pub trait SliceLike {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()>;
}

impl SliceLike for &mut [u8] {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        if offset as usize + buf.len() > self.len() {
            return Err(std::io::Error::other(anyhow::anyhow!(
                "reading {} bytes at offset {} is out of bounds for slice of len {}",
                buf.len(),
                offset,
                self.len()
            )));
        }

        buf.copy_from_slice(&self[offset as usize..][..buf.len()]);

        Ok(())
    }

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        if offset as usize + bytes.len() > self.len() {
            return Err(std::io::Error::other(anyhow::anyhow!(
                "writing {} bytes at offset {} is out of bounds for slice of len {}",
                bytes.len(),
                offset,
                self.len()
            )));
        }

        self[offset as usize..][..bytes.len()].copy_from_slice(bytes);

        Ok(())
    }
}

impl SliceLike for std::fs::File {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;

        self.read_exact(buf)?;

        Ok(())
    }

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;

        self.write_all(bytes)?;

        Ok(())
    }
}

#[allow(dead_code)]
pub struct FatFs {
    inner: Rc<RefCell<dyn SliceLike>>,

    fat_offset: u64,
    fat_size: usize,

    root_dir_offset: Option<u64>,
    root_dir_size: usize,

    pub data_offset: u64,
    data_size: usize,

    bytes_per_cluster: usize,

    bpb: bpb::Bpb,

    fat: fat::Fat,
}

impl FatFs {
    pub fn load(data: Rc<RefCell<dyn SliceLike>>) -> anyhow::Result<FatFs> {
        let mut bpb_bytes = [0; 512];

        data.borrow_mut().read_at_offset(0, &mut bpb_bytes)?;

        let bpb = bpb::Bpb::load(&bpb_bytes)?;

        let mut fat_buf = vec![0; bpb.fat_len_bytes()];

        data.borrow_mut()
            .read_at_offset(bpb.fat_offset(), &mut fat_buf)?;

        let fat = fat::Fat::new(bpb.fat_type(), &fat_buf, bpb.count_of_clusters());

        // {
        //     let eof = fat.get_eof_cluster();

        //     if fat.get_entry(0) != (eof & !0xFF) | bpb.media() as u32 {
        //         eprintln!("warning: first sector entry should have media in lowest byte");
        //     }

        //     if fat.get_entry(1) != eof {
        //         eprintln!(
        //             "warning: second sector entry should be EOF, not {:#X}",
        //             fat.get_entry(1)
        //         );
        //     }
        // }

        let fat_offset = bpb.fat_offset();
        let fat_size = bpb.fat_len_bytes();

        let root_dir_offset = bpb.root_directory_offset();
        let root_dir_size = bpb.root_dir_len_bytes();

        let data_offset = bpb.data_offset();
        let data_size = bpb.data_len_bytes();

        let bytes_per_cluster = bpb.bytes_per_cluster();

        Ok(FatFs {
            inner: data,
            fat_offset,
            fat_size,
            root_dir_offset,
            root_dir_size,
            data_offset,
            data_size,
            bytes_per_cluster,
            bpb,
            fat,
        })
    }

    pub fn bpb(&self) -> &bpb::Bpb {
        &self.bpb
    }

    pub fn fat(&self) -> &fat::Fat {
        &self.fat
    }

    /// byte offset of data cluster
    pub fn data_cluster_to_offset(&self, cluster: u32) -> u64 {
        // assert!(cluster >= 2);

        assert!(self.fat().get_valid_clusters().contains(&cluster));

        self.data_offset + (cluster - 2) as u64 * self.bytes_per_cluster as u64
    }

    /// next data cluster or None is cluster is EOF
    ///
    /// giving an invalid cluster (free, reserved, or defective) returns an appropriate error
    pub fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FatError> {
        self.fat().get_next_cluster(cluster)
    }

    pub fn cluster_as_subslice_mut(&mut self, cluster: u32) -> SubSliceMut<'_> {
        let offset = self.data_cluster_to_offset(cluster);

        SubSliceMut::new(self, offset, self.bytes_per_cluster)
    }

    pub fn cluster_as_subslice(&self, cluster: u32) -> SubSlice<'_> {
        let offset = self.data_cluster_to_offset(cluster);

        SubSlice::new(self, offset, self.bytes_per_cluster)
    }

    pub fn root_dir_bytes(&mut self) -> std::io::Result<Vec<u8>> {
        if let Some(root_dir_offset) = self.root_dir_offset {
            let mut data = Vec::new();

            let mut subslice = SubSliceMut::new(self, root_dir_offset, self.root_dir_size);

            subslice.read_to_end(&mut data)?;

            return Ok(data);
        }

        let mut cluster = self.bpb().root_cluster().unwrap();

        let mut data = vec![0; self.bytes_per_cluster];

        let mut inner = self.inner.borrow_mut();

        inner.read_at_offset(self.data_cluster_to_offset(cluster), &mut data)?;

        while let Ok(Some(next_cluster)) = self.next_cluster(cluster) {
            cluster = next_cluster;

            inner.read_at_offset(self.data_cluster_to_offset(cluster), &mut data)?;
        }

        Ok(data)
    }

    fn chain_reader(&self, first_cluster: u32) -> impl Read {
        iter::ClusterChainReader::new(self, first_cluster)
    }

    pub fn root_dir_iter<'a>(&'a self) -> DirIter<Box<dyn Read + 'a>> {
        // Box<dyn Iterator<Item = DirEntry> + '_>
        // TODO: maybe wrap this in another RootDirIter enum, so we don't have to Box<dyn>

        if let Some(root_dir_offset) = self.root_dir_offset {
            // FAT12/FAT16

            let sub_slice = SubSlice::new(self, root_dir_offset, self.root_dir_size);

            return DirIter::new(Box::new(sub_slice));
        }

        // FAT32

        // can't fail; we're in the FAT32 case
        let root_cluster = self.bpb().root_cluster().unwrap();

        let cluster_iter = iter::ClusterChainReader::new(self, root_cluster);

        DirIter::new(Box::new(cluster_iter))
    }

    pub fn dir_iter<'a>(&'a self, first_cluster: u32) -> DirIter<Box<dyn Read + 'a>> {
        // TODO: return type must match root_dir_iter
        // if the Box<dyn> is changed there, update here as well

        let cluster_iter = self.chain_reader(first_cluster);

        DirIter::new(Box::new(cluster_iter))
    }
}

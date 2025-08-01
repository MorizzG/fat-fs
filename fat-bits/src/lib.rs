use std::cell::RefCell;
use std::fmt::Display;
use std::io::{Read, Seek, SeekFrom, Write};
use std::rc::Rc;

use crate::dir::DirIter;
use crate::fat::{FatError, FatOps};
use crate::subslice::{SubSlice, SubSliceMut};

pub mod bpb;
mod datetime;
pub mod dir;
pub mod fat;
pub mod fs_info;
pub mod iter;
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

impl Display for FatFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.bpb)?;
        writeln!(f, "")?;
        writeln!(f, "{}", self.fat)?;

        Ok(())
    }
}

unsafe impl Send for FatFs {}

impl FatFs {
    pub fn load<S>(data: S) -> anyhow::Result<FatFs>
    where
        S: SliceLike + Send + 'static,
    {
        let data = Rc::new(RefCell::new(data));

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

    /// byte offset of data cluster
    fn data_cluster_to_offset(&self, cluster: u32) -> u64 {
        // assert!(cluster >= 2);

        assert!(self.fat.valid_clusters().contains(&cluster));

        self.data_offset + (cluster - 2) as u64 * self.bytes_per_cluster as u64
    }

    pub fn free_clusters(&self) -> usize {
        self.fat.count_free_clusters()
    }

    pub fn bytes_per_sector(&self) -> u16 {
        self.bpb.bytes_per_sector()
    }

    pub fn sectors_per_cluster(&self) -> u8 {
        self.bpb.sectors_per_cluster()
    }

    pub fn root_cluster(&self) -> Option<u32> {
        self.bpb.root_cluster()
    }

    /// next data cluster or None is cluster is EOF
    ///
    /// giving an invalid cluster (free, reserved, or defective) returns an appropriate error
    pub fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FatError> {
        self.fat.get_next_cluster(cluster)
    }

    pub fn cluster_as_subslice_mut(&self, cluster: u32) -> SubSliceMut {
        if cluster == 0 {
            // for cluster 0 simply return empty subslice
            // this makes things a bit easier, since cluster 0 is used as a marker that a file/dir
            // is empty

            SubSliceMut::new(self.inner.clone(), 0, 0);
        }

        let offset = self.data_cluster_to_offset(cluster);

        SubSliceMut::new(self.inner.clone(), offset, self.bytes_per_cluster)
    }

    pub fn cluster_as_subslice(&self, cluster: u32) -> SubSlice {
        if cluster == 0 {
            // for cluster 0 simply return empty subslice
            // this makes things a bit easier, since cluster 0 is used as a marker that a file/dir
            // is empty

            SubSlice::new(self.inner.clone(), 0, 0);
        }

        let offset = self.data_cluster_to_offset(cluster);

        SubSlice::new(self.inner.clone(), offset, self.bytes_per_cluster)
    }

    fn chain_reader(&'_ self, first_cluster: u32) -> iter::ClusterChainReader<'_> {
        iter::ClusterChainReader::new(self, first_cluster)
    }

    fn chain_writer(&'_ self, first_cluster: u32) -> iter::ClusterChainWriter<'_> {
        iter::ClusterChainWriter::new(self, first_cluster)
    }

    pub fn root_dir_iter<'a>(&'a self) -> DirIter<Box<dyn Read + 'a>> {
        // Box<dyn Iterator<Item = DirEntry> + '_>
        // TODO: maybe wrap this in another RootDirIter enum, so we don't have to Box<dyn>

        if let Some(root_dir_offset) = self.root_dir_offset {
            // FAT12/FAT16

            let sub_slice = SubSlice::new(self.inner.clone(), root_dir_offset, self.root_dir_size);

            return DirIter::new(Box::new(sub_slice));
        }

        // FAT32

        // can't fail; we're in the FAT32 case
        let root_cluster = self.bpb.root_cluster().unwrap();

        let cluster_iter = iter::ClusterChainReader::new(self, root_cluster);

        DirIter::new(Box::new(cluster_iter))
    }

    pub fn dir_iter<'a>(&'a self, first_cluster: u32) -> DirIter<Box<dyn Read + 'a>> {
        // TODO: return type must match root_dir_iter
        // if the Box<dyn> is changed there, update here as well

        let cluster_iter = self.chain_reader(first_cluster);

        DirIter::new(Box::new(cluster_iter))
    }

    pub fn file_reader(&self, first_cluster: u32) -> iter::ClusterChainReader<'_> {
        // TODO: needs to take file size into account
        assert!(first_cluster >= 2);

        self.chain_reader(first_cluster)
    }

    pub fn file_writer(&self, first_cluster: u32) -> iter::ClusterChainWriter<'_> {
        // TODO: needs to take file size into account
        assert!(first_cluster >= 2);

        self.chain_writer(first_cluster)
    }
}

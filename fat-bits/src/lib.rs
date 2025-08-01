use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

use crate::dir::DirIter;
use crate::fat::{FatError, FatOps};
use crate::iter::ClusterChainReader;
pub use crate::slice_like::SliceLike;
use crate::subslice::{SubSlice, SubSliceMut};

pub mod bpb;
mod datetime;
pub mod dir;
pub mod fat;
pub mod fs_info;
pub mod iter;
mod slice_like;
mod subslice;
mod utils;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

pub struct FatFs {
    inner: Rc<RefCell<dyn SliceLike>>,

    // fat_offset: u64,
    // fat_size: usize,
    root_dir_offset: Option<u64>,
    root_dir_size: usize,

    pub data_offset: u64,
    // data_size: usize,
    bytes_per_cluster: usize,

    bpb: bpb::Bpb,

    fat: fat::Fat,

    next_free: Option<u32>,
    free_count: u32,
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

        // let fat_offset = bpb.fat_offset();
        // let fat_size = bpb.fat_len_bytes();

        let root_dir_offset = bpb.root_directory_offset();
        let root_dir_size = bpb.root_dir_len_bytes();

        let data_offset = bpb.data_offset();
        // let data_size = bpb.data_len_bytes();

        let bytes_per_cluster = bpb.bytes_per_cluster();

        let next_free = fat.first_free_cluster();
        let free_count = fat.count_free_clusters();

        Ok(FatFs {
            inner: data,
            // fat_offset,
            // fat_size,
            root_dir_offset,
            root_dir_size,
            data_offset,
            // data_size,
            bytes_per_cluster,
            bpb,
            fat,
            next_free,
            free_count,
        })
    }

    pub fn fat_type(&self) -> FatType {
        match &self.fat {
            fat::Fat::Fat12(_) => FatType::Fat12,
            fat::Fat::Fat16(_) => FatType::Fat16,
            fat::Fat::Fat32(_) => FatType::Fat32,
        }
    }

    /// byte offset of data cluster
    fn data_cluster_to_offset(&self, cluster: u32) -> u64 {
        // assert!(cluster >= 2);

        assert!(self.fat.valid_entries().contains(&cluster));

        self.data_offset + (cluster - 2) as u64 * self.bytes_per_cluster as u64
    }

    pub fn free_clusters(&self) -> u32 {
        // self.fat.count_free_clusters()
        self.free_count
    }

    pub fn alloc_cluster(&mut self) -> Option<u32> {
        let Some(cluster) = self.next_free else {
            // no free cluster
            return None;
        };

        // set cluster as taken
        self.fat.set_entry(cluster, self.fat.eof_entry());

        // something went terribly wrong
        assert_ne!(self.free_count, 0);

        self.free_count -= 1;

        // find next free cluster
        self.next_free = self.fat.first_free_cluster();

        Some(cluster)
    }

    pub fn dealloc_cluster(&mut self, cluster: u32) {
        // assert cluster is actually valid
        assert!(
            self.fat
                .valid_entries()
                .contains(&self.fat.get_entry(cluster))
        );

        self.fat.set_entry(cluster, 0);

        if self.next_free.is_none() || self.next_free.unwrap() > cluster {
            self.next_free = Some(cluster);
        }

        self.free_count += 1;
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

    pub fn cluster_as_subslice(&self, cluster: u32) -> SubSlice {
        if cluster == 0 {
            // for cluster 0 simply return empty subslice
            // this makes things a bit easier, since cluster 0 is used as a marker that a file/dir
            // is empty

            return SubSlice::new(self.inner.clone(), 0, 0);
        }

        let offset = self.data_cluster_to_offset(cluster);

        SubSlice::new(self.inner.clone(), offset, self.bytes_per_cluster)
    }

    pub fn cluster_as_subslice_mut(&self, cluster: u32) -> SubSliceMut {
        if cluster == 0 {
            // for cluster 0 simply return empty subslice
            // this makes things a bit easier, since cluster 0 is used as a marker that a file/dir
            // is empty

            return SubSliceMut::new(self.inner.clone(), 0, 0);
        }

        let offset = self.data_cluster_to_offset(cluster);

        SubSliceMut::new(self.inner.clone(), offset, self.bytes_per_cluster)
    }

    fn root_dir_as_subslice(&self) -> SubSlice {
        SubSlice::new(self.inner.clone(), self.root_dir_offset.unwrap(), self.root_dir_size)
    }

    fn root_dir_as_subslice_mut(&self) -> SubSliceMut {
        SubSliceMut::new(self.inner.clone(), self.root_dir_offset.unwrap(), self.root_dir_size)
    }

    fn chain_reader(&'_ self, first_cluster: u32) -> iter::ClusterChainReader<'_> {
        iter::ClusterChainReader::new(self, first_cluster)
    }

    fn chain_writer(&'_ self, first_cluster: u32) -> iter::ClusterChainWriter<'_> {
        iter::ClusterChainWriter::new(self, first_cluster)
    }

    pub fn root_dir_iter<'a>(&self) -> DirIter<'_> {
        let reader = ClusterChainReader::root_dir_reader(self);

        DirIter::new(reader)
    }

    pub fn dir_iter<'a>(&self, first_cluster: u32) -> DirIter<'_> {
        let cluster_iter = self.chain_reader(first_cluster);

        DirIter::new(cluster_iter)
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

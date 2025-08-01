use std::io::{Read, Write};

use crate::subslice::{SubSlice, SubSliceMut};
use crate::{FatFs, FatType};

pub struct ClusterChainReader<'a> {
    fat_fs: &'a FatFs,

    sub_slice: SubSlice,

    next_cluster: Option<u32>,
}

impl<'a> ClusterChainReader<'a> {
    pub fn new(fat_fs: &'a FatFs, first_cluster: u32) -> Self {
        let next_cluster = fat_fs.next_cluster(first_cluster).unwrap_or(None);

        let sub_slice = fat_fs.cluster_as_subslice(first_cluster);

        ClusterChainReader {
            fat_fs,
            sub_slice,
            next_cluster,
        }
    }

    pub fn root_dir_reader(fat_fs: &'a FatFs) -> Self {
        match fat_fs.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // fixed root dir, so no need to chain
                // get a single SubSlice for it and next_cluster is None

                let sub_slice = fat_fs.root_dir_as_subslice();

                ClusterChainReader {
                    fat_fs,
                    sub_slice,
                    next_cluster: None,
                }
            }
            FatType::Fat32 => {
                // FAT is directory_like, so get a real chain reader

                Self::new(fat_fs, fat_fs.bpb.root_cluster().unwrap())
            }
        }
    }

    fn move_to_next_cluster(&mut self) -> bool {
        let Some(next_cluster) = self.next_cluster else {
            return false;
        };

        self.next_cluster = self.fat_fs.next_cluster(next_cluster).unwrap_or(None);
        self.sub_slice = self.fat_fs.cluster_as_subslice(next_cluster);

        true
    }

    pub fn skip(&mut self, n: u64) -> u64 {
        let mut bytes_to_skip = n;

        while bytes_to_skip > self.sub_slice.len() as u64 {
            bytes_to_skip -= self.sub_slice.len() as u64;
            if !self.move_to_next_cluster() {
                // ran out of bytes to seek
                return n - bytes_to_skip;
            }
        }

        if bytes_to_skip != 0 {
            bytes_to_skip -= self.sub_slice.skip(bytes_to_skip as usize) as u64;
        }

        // n should absolutely be zero here
        assert_eq!(bytes_to_skip, 0);

        n
    }

    pub fn current_offset(&self) -> u64 {
        self.sub_slice.offset()
    }
}

impl Read for ClusterChainReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.sub_slice.is_empty() {
            if !self.move_to_next_cluster() {
                return Ok(0);
            }
        }

        self.sub_slice.read(buf)
    }
}

pub struct ClusterChainWriter<'a> {
    fat_fs: &'a FatFs,

    sub_slice: SubSliceMut,

    next_cluster: Option<u32>,
}

impl<'a> ClusterChainWriter<'a> {
    pub fn new(fat_fs: &'a FatFs, first_cluster: u32) -> Self {
        let next_cluster = fat_fs.next_cluster(first_cluster).unwrap_or(None);

        let sub_slice = fat_fs.cluster_as_subslice_mut(first_cluster);

        ClusterChainWriter {
            fat_fs,
            sub_slice,
            next_cluster,
        }
    }

    pub fn root_dir_writer(fat_fs: &'a FatFs) -> Self {
        match fat_fs.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // fixed root dir, so no need to chain
                // get a single SubSliceMut for it and next_cluster is None

                let sub_slice = fat_fs.root_dir_as_subslice_mut();

                ClusterChainWriter {
                    fat_fs,
                    sub_slice,
                    next_cluster: None,
                }
            }
            FatType::Fat32 => {
                // FAT is directory_like, so get a real chain writer

                Self::new(fat_fs, fat_fs.bpb.root_cluster().unwrap())
            }
        }
    }

    fn move_to_next_cluster(&mut self) -> bool {
        // TODO: should allocate a new cluster here!
        let Some(next_cluster) = self.next_cluster else {
            return false;
        };

        self.next_cluster = self.fat_fs.next_cluster(next_cluster).unwrap_or(None);
        self.fat_fs.cluster_as_subslice_mut(next_cluster);

        true
    }

    pub fn skip(&mut self, n: u64) -> u64 {
        let mut bytes_to_skip = n;

        while bytes_to_skip > self.sub_slice.len() as u64 {
            bytes_to_skip -= self.sub_slice.len() as u64;
            if !self.move_to_next_cluster() {
                // ran out of bytes to seek
                return n - bytes_to_skip;
            }
        }

        if bytes_to_skip != 0 {
            bytes_to_skip -= self.sub_slice.skip(bytes_to_skip as usize) as u64;
        }

        // n should absolutely be zero here
        assert_eq!(bytes_to_skip, 0);

        n
    }

    pub fn current_offset(&self) -> u64 {
        self.sub_slice.offset()
    }
}

impl Write for ClusterChainWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.sub_slice.is_empty() {
            if !(self.move_to_next_cluster()) {
                return Ok(0);
            }
        }

        self.sub_slice.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

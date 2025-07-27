use std::io::Read;

use crate::FatFs;
use crate::subslice::SubSlice;
use crate::utils::replace;

pub struct ClusterChainReader<'a> {
    sub_slice: SubSlice<'a>,

    next_cluster: Option<u32>,
}

impl<'a> ClusterChainReader<'a> {
    pub fn new(fat_fs: &'a FatFs, first_cluster: u32) -> ClusterChainReader<'a> {
        let next_cluster = fat_fs.next_cluster(first_cluster).unwrap_or(None);

        let sub_slice = fat_fs.cluster_as_subslice(first_cluster);

        ClusterChainReader {
            sub_slice,
            next_cluster,
        }
    }

    fn next_cluster(&mut self) -> bool {
        let Some(next_cluster) = self.next_cluster else {
            return false;
        };

        replace(&mut self.sub_slice, |sub_slice| {
            let fat_fs = sub_slice.release();

            self.next_cluster = fat_fs.next_cluster(next_cluster).unwrap_or(None);

            fat_fs.cluster_as_subslice(next_cluster)
        });

        true
    }
}

impl<'a> Read for ClusterChainReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.sub_slice.is_empty() {
            if !self.next_cluster() {
                return Ok(0);
            }
        }

        self.sub_slice.read(buf)
    }
}

use std::io::Read;

use crate::subslice::SubSlice;
use crate::utils::replace;
use crate::{FatFs, SliceLike};

pub struct ClusterChainReader<'a, S: SliceLike> {
    sub_slice: SubSlice<'a, S>,

    next_cluster: Option<u32>,
}

impl<'a, S: SliceLike> ClusterChainReader<'a, S> {
    pub fn new(fat_fs: &'a FatFs<S>, first_cluster: u32) -> ClusterChainReader<'a, S> {
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

impl<'a, S: SliceLike> Read for ClusterChainReader<'a, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.sub_slice.is_empty() {
            if !self.next_cluster() {
                return Ok(0);
            }
        }

        self.sub_slice.read(buf)
    }
}

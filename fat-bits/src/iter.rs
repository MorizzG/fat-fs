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

    fn move_to_next_cluster(&mut self) -> bool {
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

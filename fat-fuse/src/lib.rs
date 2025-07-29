mod fuse;
mod inode;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use fat_bits::{FatFs, SliceLike};

use crate::inode::Inode;

#[allow(dead_code)]
pub struct FatFuse {
    fat_fs: FatFs,

    uid: u32,
    gid: u32,

    next_fd: u32,

    inode_table: BTreeMap<u64, Inode>,
}

impl FatFuse {
    pub fn new(data: Rc<RefCell<dyn SliceLike>>) -> anyhow::Result<FatFuse> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let fat_fs = FatFs::load(data)?;

        Ok(FatFuse {
            fat_fs,
            uid,
            gid,
            next_fd: 0,
            inode_table: BTreeMap::new(),
        })
    }

    fn get_inode(&self, ino: u64) -> Option<&Inode> {
        self.inode_table.get(&ino)
    }

    fn get_inode_mut(&mut self, ino: u64) -> Option<&mut Inode> {
        self.inode_table.get_mut(&ino)
    }
}

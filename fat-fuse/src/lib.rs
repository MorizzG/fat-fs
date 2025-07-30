mod fuse;
mod inode;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use fat_bits::{FatFs, SliceLike};
use log::debug;

use crate::inode::Inode;

#[allow(dead_code)]
pub struct FatFuse {
    fat_fs: FatFs,

    uid: u32,
    gid: u32,

    next_ino: u64,
    next_fd: u32,

    inode_table: BTreeMap<u64, Inode>,

    ino_by_first_cluster: BTreeMap<u32, u64>,
}

impl FatFuse {
    pub fn new(data: Rc<RefCell<dyn SliceLike>>) -> anyhow::Result<FatFuse> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let fat_fs = FatFs::load(data)?;

        // TODO: build and insert root dir inode

        Ok(FatFuse {
            fat_fs,
            uid,
            gid,
            next_ino: 2, // 0 is reserved and 1 is root
            next_fd: 0,
            inode_table: BTreeMap::new(),
            ino_by_first_cluster: BTreeMap::new(),
        })
    }

    fn next_ino(&mut self) -> u64 {
        let ino = self.next_ino;

        assert!(!self.inode_table.contains_key(&ino));

        self.next_ino += 1;

        ino
    }

    fn insert_inode(&mut self, inode: Inode) -> &mut Inode {
        let ino = inode.ino();
        let generation = inode.generation();
        let first_cluster = inode.first_cluster();

        // let old_inode = self.inode_table.insert(ino, inode);

        let entry = self.inode_table.entry(ino);

        let (new_inode, old_inode): (&mut Inode, Option<Inode>) = match entry {
            std::collections::btree_map::Entry::Vacant(vacant_entry) => {
                let new_inode = vacant_entry.insert(inode);
                (new_inode, None)
            }
            std::collections::btree_map::Entry::Occupied(occupied_entry) => {
                let entry_ref = occupied_entry.into_mut();

                let old_inode = std::mem::replace(entry_ref, inode);

                (entry_ref, Some(old_inode))
            }
        };

        let old_ino = self.ino_by_first_cluster.insert(first_cluster, ino);

        debug!(
            "inserted new inode with ino {} and generation {} (first cluster: {})",
            ino, generation, first_cluster
        );

        if let Some(old_inode) = old_inode {
            debug!("ejected inode {} {}", old_inode.ino(), old_inode.generation());
        }

        if let Some(old_ino) = old_ino {
            debug!("ejected old {} -> {} cluster to ino mapping", first_cluster, old_ino);
        }

        new_inode
    }

    fn drop_inode(&mut self, ino: u64) {
        let Some(inode) = self.inode_table.remove(&ino) else {
            debug!("tried to drop inode with ino {}, but was not in table", ino);

            return;
        };

        let first_cluster = inode.first_cluster();

        let entry = self.ino_by_first_cluster.entry(first_cluster);

        match entry {
            std::collections::btree_map::Entry::Vacant(_) => debug!(
                "removed inode with ino {} from table, but it's first cluster did not point to any ino",
                ino
            ),
            std::collections::btree_map::Entry::Occupied(occupied_entry) => {
                let found_ino = *occupied_entry.get();

                if found_ino == ino {
                    // matches our inode, remove it
                    occupied_entry.remove();
                } else {
                    // does not match removed inode, leave it as is
                    debug!(
                        "removed inode with ino {} from table, but it's first cluster pointer to ino {} instead",
                        ino, found_ino
                    );
                }
            }
        }

        todo!()
    }

    fn get_inode(&self, ino: u64) -> Option<&Inode> {
        self.inode_table.get(&ino)
    }

    fn get_inode_mut(&mut self, ino: u64) -> Option<&mut Inode> {
        self.inode_table.get_mut(&ino)
    }

    pub fn get_inode_by_first_cluster(&self, first_cluster: u32) -> Option<&Inode> {
        let ino = self.ino_by_first_cluster.get(&first_cluster)?;

        if let Some(inode) = self.inode_table.get(ino) {
            Some(inode)
        } else {
            debug!(
                "first cluster {} is mapped to ino {}, but inode is not in table",
                first_cluster, ino
            );

            None
        }
    }

    pub fn get_inode_by_first_cluster_mut(&mut self, first_cluster: u32) -> Option<&mut Inode> {
        let ino = self.ino_by_first_cluster.get(&first_cluster)?;

        if let Some(inode) = self.inode_table.get_mut(ino) {
            Some(inode)
        } else {
            debug!(
                "first cluster {} is mapped to ino {}, but inode is not in table",
                first_cluster, ino
            );

            None
        }
    }
}

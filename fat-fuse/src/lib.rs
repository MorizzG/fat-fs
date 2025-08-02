mod fuse;
mod inode;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use fat_bits::dir::DirEntry;
use fat_bits::{FatFs, SliceLike};
use fxhash::FxHashMap;
use log::{debug, error};

use crate::inode::{Inode, InodeRef};

#[allow(dead_code)]
pub struct FatFuse {
    fat_fs: FatFs,

    uid: u32,
    gid: u32,

    next_ino: u64,
    next_fh: u64,

    inode_table: BTreeMap<u64, InodeRef>,

    ino_by_first_cluster: BTreeMap<u32, u64>,
    ino_by_fh: BTreeMap<u64, u64>,
    ino_by_path: FxHashMap<Rc<str>, u64>,
}

/// SAFETY
///
/// do NOT leak Rc<str> from this type
unsafe impl Send for FatFuse {}

impl FatFuse {
    pub fn new<S>(data: S) -> anyhow::Result<FatFuse>
    where
        S: SliceLike + Send + 'static,
    {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let fat_fs = FatFs::load(data)?;

        let mut fat_fuse = FatFuse {
            fat_fs,
            uid,
            gid,
            next_ino: 2, // 0 is reserved and 1 is root
            next_fh: 0,
            inode_table: BTreeMap::new(),
            ino_by_first_cluster: BTreeMap::new(),
            ino_by_fh: BTreeMap::new(),
            ino_by_path: FxHashMap::default(),
        };

        // TODO: build and insert root dir inode

        let root_inode = Inode::root_inode(&fat_fuse.fat_fs, uid, gid);

        fat_fuse.insert_inode(root_inode);

        Ok(fat_fuse)
    }

    fn next_ino(&mut self) -> u64 {
        let ino = self.next_ino;

        assert!(!self.inode_table.contains_key(&ino));

        self.next_ino += 1;

        ino
    }

    fn next_fh(&mut self) -> u64 {
        let fh = self.next_fh;

        assert!(!self.ino_by_fh.contains_key(&fh));

        self.next_fh += 1;

        fh
    }

    fn insert_inode(&mut self, inode: Inode) -> InodeRef {
        let ino = inode.ino();
        let generation = inode.generation();
        let first_cluster = inode.first_cluster();

        // let old_inode = self.inode_table.insert(ino, inode);

        let inode = Rc::new(RefCell::new(inode));

        let entry = self.inode_table.entry(ino);

        let (new_inode, old_inode) = match entry {
            std::collections::btree_map::Entry::Vacant(vacant_entry) => {
                let new_inode = vacant_entry.insert(inode);
                (Rc::clone(new_inode), None)
            }
            std::collections::btree_map::Entry::Occupied(occupied_entry) => {
                let entry_ref = occupied_entry.into_mut();

                let old_inode = std::mem::replace(entry_ref, inode);

                (Rc::clone(entry_ref), Some(old_inode))
            }
        };

        debug!(
            "inserted new inode with ino {} and generation {} (first cluster: {})",
            ino, generation, first_cluster
        );

        if let Some(old_inode) = old_inode {
            let old_inode = old_inode.borrow();

            debug!("ejected inode {} {}", old_inode.ino(), old_inode.generation());
        }

        if first_cluster != 0 {
            if let Some(old_ino) = self.ino_by_first_cluster.insert(first_cluster, ino) {
                debug!("ejected old {} -> {} cluster to ino mapping", first_cluster, old_ino);
            }
        }

        let path = new_inode.borrow().path();

        if let Some(old_ino) = self.ino_by_path.insert(Rc::clone(&path), ino) {
            debug!("ejected old {} -> {} path to ino mapping", path, old_ino);
        }

        new_inode
    }

    fn drop_inode(&mut self, inode: InodeRef) {
        let inode = inode.borrow();

        let ino = inode.ino();

        debug!("dropping inode {}", ino);

        if self.inode_table.remove(&ino).is_none() {
            error!("tried to drop inode with ino {}, but was not in table", ino);

            return;
        };

        let first_cluster = inode.first_cluster();

        if first_cluster != 0 {
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
                            "removed inode with ino {} from table, but its first cluster pointed to ino {} instead",
                            ino, found_ino
                        );
                    }
                }
            }
        }

        {
            let entry = self.ino_by_path.entry(inode.path());

            match entry {
                std::collections::hash_map::Entry::Vacant(_) => debug!(
                    "removed inode with ino {} from table, but it's path did not point to any ino",
                    ino
                ),
                std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                    let found_ino = *occupied_entry.get();

                    if found_ino == ino {
                        // matches our inode, remove it
                        occupied_entry.remove();
                    } else {
                        // does not match removed inode, leave it as is
                        debug!(
                            "removed inode with ino {} from table, but its path pointed to ino {} instead",
                            ino, found_ino
                        );
                    }
                }
            }
        }
    }

    fn get_inode(&self, ino: u64) -> Option<&InodeRef> {
        self.inode_table.get(&ino)
    }

    fn get_or_make_inode(&mut self, dir_entry: &DirEntry, parent: &Inode) -> InodeRef {
        // let parent = parent.borrow();

        // try to find inode by first cluster first
        if dir_entry.first_cluster() != 0
            && let Some(inode) = self.get_inode_by_first_cluster(dir_entry.first_cluster())
        {
            return inode;
        }

        // try to find inode by path
        // mostly for empty files/directories which have a first cluster of 0

        let path = {
            let mut path = parent.path().as_ref().to_owned();

            if parent.ino() != inode::ROOT_INO {
                // root inode already has trailing slash
                path.push('/');
            }

            path += &dir_entry.name_string();

            path
        };

        if let Some(inode) = self.get_inode_by_path(&path) {
            return inode;
        }

        // no inode found, make a new one
        let ino = self.next_ino();

        let Some(parent_inode) = self.get_inode(parent.ino()).cloned() else {
            // TODO: what do we do here? should not happen
            panic!("parent_ino {} does not lead to inode", parent.ino());
        };

        let inode =
            Inode::new(&self.fat_fs, dir_entry, ino, self.uid, self.gid, path, parent_inode);

        self.insert_inode(inode)
    }

    pub fn get_inode_by_first_cluster(&self, first_cluster: u32) -> Option<InodeRef> {
        if first_cluster == 0 {
            debug!("trying to get inode by first cluster 0");

            return None;
        }

        let ino = self.ino_by_first_cluster.get(&first_cluster)?;

        if let Some(inode) = self.inode_table.get(ino) {
            Some(Rc::clone(inode))
        } else {
            debug!(
                "first cluster {} is mapped to ino {}, but inode is not in table",
                first_cluster, ino
            );

            None
        }
    }

    pub fn get_inode_by_fh(&self, fh: u64) -> Option<&InodeRef> {
        let ino = *self.ino_by_fh.get(&fh)?;

        if let Some(inode) = self.get_inode(ino) {
            Some(inode)
        } else {
            debug!("fh {} is mapped to ino {}, but inode is not in table", fh, ino);

            None
        }
    }

    pub fn get_inode_by_path(&self, path: &str) -> Option<InodeRef> {
        let ino = *self.ino_by_path.get(path)?;

        if let Some(inode) = self.get_inode(ino).cloned() {
            Some(inode)
        } else {
            debug!("path {} is mapped to ino {}, but inode is not in table", path, ino);

            None
        }
    }
}

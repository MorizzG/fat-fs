mod fuse;
mod inode;

use std::collections::BTreeMap;
use std::rc::Rc;

use fat_bits::dir::DirEntry;
use fat_bits::{FatFs, SliceLike};
use fxhash::FxHashMap;
use log::debug;

use crate::inode::Inode;

#[allow(dead_code)]
pub struct FatFuse {
    fat_fs: FatFs,

    uid: u32,
    gid: u32,

    next_ino: u64,
    next_fh: u64,

    inode_table: BTreeMap<u64, Inode>,

    ino_by_first_cluster: BTreeMap<u32, u64>,
    ino_by_fh: BTreeMap<u64, u64>,
    ino_by_path: FxHashMap<Rc<str>, u64>,
}

impl Drop for FatFuse {
    fn drop(&mut self) {
        println!("inode_table: {}", self.inode_table.len());

        println!("ino_by_first_cluster: {}", self.ino_by_first_cluster.len());
        for (&first_cluster, &ino) in self.ino_by_first_cluster.iter() {
            println!("{} -> {}", first_cluster, ino);
        }

        println!("ino_by_fh: {}", self.ino_by_fh.len());

        println!("ino_by_path: {}", self.ino_by_path.len());
    }
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

        debug!(
            "inserted new inode with ino {} and generation {} (first cluster: {})",
            ino, generation, first_cluster
        );

        if let Some(old_inode) = old_inode {
            debug!("ejected inode {} {}", old_inode.ino(), old_inode.generation());
        }

        if first_cluster != 0 {
            if let Some(old_ino) = self.ino_by_first_cluster.insert(first_cluster, ino) {
                debug!("ejected old {} -> {} cluster to ino mapping", first_cluster, old_ino);
            }
        }

        if let Some(old_ino) = self.ino_by_path.insert(new_inode.path(), ino) {
            debug!("ejected old {} -> {} path to ino mapping", new_inode.path(), old_ino);
        }

        new_inode
    }

    fn drop_inode(&mut self, ino: u64) {
        debug!("dropping ino {}", ino);

        let Some(inode) = self.inode_table.remove(&ino) else {
            debug!("tried to drop inode with ino {}, but was not in table", ino);

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

        let Some(parent_inode) = self.get_inode_mut(inode.parent_ino()) else {
            panic!("parent inode {} does not exists anymore", inode.parent_ino());
        };

        // dec refcount
        parent_inode.dec_ref_count(1);
    }

    fn get_inode(&self, ino: u64) -> Option<&Inode> {
        self.inode_table.get(&ino)
    }

    fn get_inode_mut(&mut self, ino: u64) -> Option<&mut Inode> {
        self.inode_table.get_mut(&ino)
    }

    fn get_or_make_inode_by_dir_entry(
        &mut self,
        dir_entry: &DirEntry,
        parent_ino: u64,
        parent_path: Rc<str>,
    ) -> &mut Inode {
        if dir_entry.first_cluster() != 0
            && self
                .get_inode_by_first_cluster_mut(dir_entry.first_cluster())
                .is_some()
        {
            return self
                .get_inode_by_first_cluster_mut(dir_entry.first_cluster())
                .unwrap();
        }

        let path = {
            let mut path = parent_path.as_ref().to_owned();

            if parent_ino != inode::ROOT_INO {
                // root inode already has trailing slash
                path.push('/');
            }

            path += &dir_entry.name_string();

            path
        };

        if self.get_inode_by_path_mut(&path).is_some() {
            return self.get_inode_by_path_mut(&path).unwrap();
        }

        // no inode found, make a new one
        let ino = self.next_ino();

        let Some(parent_inode) = self.get_inode_mut(parent_ino) else {
            // TODO: what do we do here? should not happen
            panic!("parent_ino {} does not lead to inode", parent_ino);
        };

        // inc ref of parent
        parent_inode.inc_ref_count();

        let inode = Inode::new(&self.fat_fs, dir_entry, ino, self.uid, self.gid, path, parent_ino);

        self.insert_inode(inode)
    }

    pub fn get_inode_by_first_cluster(&self, first_cluster: u32) -> Option<&Inode> {
        if first_cluster == 0 {
            debug!("trying to get inode by first cluster 0");

            return None;
        }

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
        if first_cluster == 0 {
            debug!("trying to get inode by first cluster 0");

            return None;
        }

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

    pub fn get_inode_by_fh(&self, fh: u64) -> Option<&Inode> {
        let ino = *self.ino_by_fh.get(&fh)?;

        if let Some(inode) = self.get_inode(ino) {
            Some(inode)
        } else {
            debug!("fh {} is mapped to ino {}, but inode is not in table", fh, ino);

            None
        }
    }

    pub fn get_inode_by_fh_mut(&mut self, fh: u64) -> Option<&mut Inode> {
        let ino = *self.ino_by_fh.get(&fh)?;

        if let Some(inode) = self.get_inode_mut(ino) {
            Some(inode)
        } else {
            debug!("fh {} is mapped to ino {}, but inode is not in table", fh, ino);

            None
        }
    }

    pub fn get_inode_by_path(&self, path: &str) -> Option<&Inode> {
        let ino = *self.ino_by_path.get(path)?;

        if let Some(inode) = self.get_inode(ino) {
            Some(inode)
        } else {
            debug!("path {} is mapped to ino {}, but inode is not in table", path, ino);

            None
        }
    }

    pub fn get_inode_by_path_mut(&mut self, path: &str) -> Option<&mut Inode> {
        let ino = *self.ino_by_path.get(path)?;

        if let Some(inode) = self.get_inode_mut(ino) {
            Some(inode)
        } else {
            debug!("path {} is mapped to ino {}, but inode is not in table", path, ino);

            None
        }
    }
}

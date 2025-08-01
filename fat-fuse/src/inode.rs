use std::cell::{LazyCell, RefCell};
use std::rc::Rc;
use std::time::SystemTime;

use chrono::{NaiveDateTime, NaiveTime};
use fat_bits::FatFs;
use fat_bits::dir::DirEntry;
use fat_bits::iter::{ClusterChainReader, ClusterChainWriter};
use fuser::FileAttr;
use libc::{EISDIR, ENOTDIR};
use log::debug;
use rand::{Rng, SeedableRng as _};

thread_local! {
/// SAFETY
///
/// do not access this directly, only invoke the get_random_u32 function
// static RNG: LazyCell<UnsafeCell<rand::rngs::SmallRng>> = LazyCell::new(|| UnsafeCell::new(rand::rngs::SmallRng::from_os_rng()));

/// performance should not be a bottleneck here, since we only need to occasionally generate u32s to
/// be used as generations in inodes
/// if at some point (contrary to expectations) it should become, can switch it to an UnsafeCell
static RNG: LazyCell<RefCell<rand::rngs::SmallRng>> = LazyCell::new(|| RefCell::new(rand::rngs::SmallRng::from_os_rng()));
}

fn get_random<T>() -> T
where
    rand::distr::StandardUniform: rand::distr::Distribution<T>,
{
    RNG.with(|rng| rng.borrow_mut().random())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

impl From<Kind> for fuser::FileType {
    fn from(value: Kind) -> Self {
        match value {
            Kind::File => fuser::FileType::RegularFile,
            Kind::Dir => fuser::FileType::Directory,
        }
    }
}

pub const ROOT_INO: u64 = 1;

pub type InodeRef = Rc<RefCell<Inode>>;
// pub type InodeWeak = Weak<RefCell<Inode>>;

#[derive(Debug)]
#[allow(dead_code)]
pub struct Inode {
    ino: u64,
    // FUSE uses a u64 for generation, but the Linux kernel only handles u32s anyway, truncating
    // the high bits, so using more is pretty pointless and possibly even detrimental
    generation: u32,

    ref_count: u64,

    parent: Option<InodeRef>,

    size: u64,
    block_size: u32,

    kind: Kind,

    read_only: bool,

    atime: SystemTime,
    mtime: SystemTime,
    // ctime: SystemTime,
    crtime: SystemTime,

    uid: u32,
    gid: u32,

    first_cluster: u32,

    path: Rc<str>,
}

#[allow(dead_code)]
impl Inode {
    fn new_generation() -> u32 {
        let rand: u16 = get_random();

        let secs = SystemTime::UNIX_EPOCH
            .elapsed()
            .map(|dur| dur.as_secs() as u16)
            .unwrap_or(0);

        ((secs as u32) << 16) | rand as u32
    }

    pub fn new(
        fat_fs: &FatFs,
        dir_entry: &DirEntry,
        ino: u64,
        uid: u32,
        gid: u32,
        path: impl Into<Rc<str>>,
        parent: InodeRef,
    ) -> Inode {
        assert!(dir_entry.is_file() || dir_entry.is_dir());

        let generation = Self::new_generation();

        let kind = if dir_entry.is_dir() {
            Kind::Dir
        } else {
            Kind::File
        };

        let datetime_to_system = |datetime: NaiveDateTime| -> SystemTime {
            datetime
                .and_local_timezone(chrono::Local)
                .single()
                .map(|x| -> SystemTime { x.into() })
                .unwrap_or(SystemTime::UNIX_EPOCH)
        };

        let atime = datetime_to_system(dir_entry.last_access_date().and_time(NaiveTime::default()));
        let mtime = datetime_to_system(dir_entry.write_time());
        let crtime = datetime_to_system(dir_entry.create_time());

        let path = path.into();

        debug!(
            "creating new inode: ino: {}    name: {}    path: {}",
            ino,
            dir_entry.name_string(),
            path
        );

        Inode {
            ino,
            generation,
            ref_count: 0,
            parent: Some(parent),
            size: dir_entry.file_size() as u64,
            block_size: fat_fs.bytes_per_sector() as u32,
            kind,
            read_only: dir_entry.is_readonly(),
            atime,
            mtime,
            crtime,
            uid,
            gid,
            first_cluster: dir_entry.first_cluster(),
            path,
        }
    }

    pub fn root_inode(fat_fs: &FatFs, uid: u32, gid: u32) -> Inode {
        let root_cluster = fat_fs.root_cluster().unwrap_or(0);

        Inode {
            ino: ROOT_INO,
            generation: 0, // root cluster always has constant generation of 0
            ref_count: 0,
            parent: None, // parent is self
            size: 0,
            block_size: fat_fs.bytes_per_sector() as u32,
            kind: Kind::Dir,
            read_only: false,
            atime: SystemTime::UNIX_EPOCH,
            mtime: SystemTime::UNIX_EPOCH,
            crtime: SystemTime::UNIX_EPOCH,
            uid,
            gid,
            first_cluster: root_cluster,
            path: "/".into(),
        }
    }

    pub fn ino(&self) -> u64 {
        self.ino
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn ref_count(&self) -> u64 {
        self.ref_count
    }

    pub fn inc_ref_count(&mut self) {
        debug!(
            "increasing ref_count of ino {} by 1 (new ref_count: {})",
            self.ino(),
            self.ref_count() + 1
        );

        self.ref_count += 1;
    }

    pub fn dec_ref_count(&mut self, n: u64) -> u64 {
        debug!(
            "decreasing ref_count of ino {} by {} (new ref_count: {})",
            self.ino(),
            n,
            self.ref_count().saturating_sub(n),
        );

        if self.ref_count < n {
            debug!(
                "inode {}: tried to decrement refcount by {}, but is only {}",
                self.ino(),
                n,
                self.ref_count
            );
        }

        self.ref_count = self.ref_count.saturating_sub(n);

        self.ref_count
    }

    pub fn parent(&self) -> Option<&InodeRef> {
        self.parent.as_ref()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn update_size(&mut self, fat_fs: &FatFs, new_size: u64) -> anyhow::Result<()> {
        let Some(parent_inode) = self.parent() else {
            anyhow::bail!("parent inode of {} does not exist", self.ino);
        };

        let parent_inode = parent_inode.borrow();

        // since we just wrote to the file with this inode, first cluster should not be zero
        let Some(mut dir_entry) = parent_inode
            .dir_iter(fat_fs)
            .unwrap()
            .find(|dir_entry| dir_entry.first_cluster() == self.first_cluster())
        else {
            anyhow::bail!("could not find dir_entry corresponding to self in parent inode");
        };

        debug!("new file size: {new_size}");

        assert!(new_size <= u32::MAX as u64);

        dir_entry.update_file_size(new_size as u32);

        if dir_entry.update(fat_fs).is_err() {
            anyhow::bail!("failed to update dir_entry for inode {}", self.ino);
        }

        drop(parent_inode);

        self.size = new_size;

        Ok(())
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn is_file(&self) -> bool {
        self.kind == Kind::File
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn first_cluster(&self) -> u32 {
        self.first_cluster
    }

    pub fn path(&self) -> Rc<str> {
        Rc::clone(&self.path)
    }

    pub fn is_root(&self) -> bool {
        self.ino == ROOT_INO
    }

    pub fn file_attr(&self) -> FileAttr {
        let perm = if self.read_only { 0o555 } else { 0o777 };

        FileAttr {
            ino: self.ino,
            size: self.size,
            blocks: self.size / self.block_size as u64,
            atime: self.atime,
            mtime: self.mtime,
            ctime: self.mtime,
            crtime: self.crtime,
            kind: self.kind.into(),
            perm,
            nlink: 1,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: self.block_size,
            flags: 0,
        }
    }

    pub fn dir_iter(&self, fat_fs: &FatFs) -> Result<impl Iterator<Item = DirEntry>, i32> {
        if self.kind != Kind::Dir {
            return Err(ENOTDIR);
        }

        if self.is_root() {
            // root dir

            return Ok(fat_fs.root_dir_iter());
        }

        Ok(fat_fs.dir_iter(self.first_cluster))
    }

    pub fn file_reader<'a>(&'a self, fat_fs: &'a FatFs) -> Result<ClusterChainReader<'a>, i32> {
        if self.is_dir() {
            return Err(EISDIR);
        }

        Ok(fat_fs.file_reader(self.first_cluster()))
    }

    pub fn file_writer<'a>(&'a self, fat_fs: &'a FatFs) -> Result<ClusterChainWriter<'a>, i32> {
        if self.is_dir() {
            return Err(EISDIR);
        }

        Ok(fat_fs.file_writer(self.first_cluster()))
    }

    // pub fn write_back(&self, fat_fs: &FatFs) {
    //     // let

    //     todo!()
    // }
}

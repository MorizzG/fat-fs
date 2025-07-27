use std::time::SystemTime;

use chrono::{NaiveDateTime, NaiveTime};
use fat_bits::FatFs;
use fat_bits::dir::DirEntry;
use fuser::FileAttr;

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
#[allow(dead_code)]
pub struct Inode {
    ino: u64,

    size: u64,
    // blocks: u64,
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
}

#[allow(dead_code)]
impl Inode {
    pub fn new(fat_fs: &FatFs, dir_entry: DirEntry, uid: u32, gid: u32) -> Inode {
        assert!(dir_entry.is_file() || dir_entry.is_dir());

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

        Inode {
            ino: dir_entry.first_cluster() as u64,
            size: dir_entry.file_size() as u64,
            block_size: fat_fs.bpb().bytes_per_sector() as u32,
            kind,
            read_only: dir_entry.is_readonly(),
            atime,
            mtime,
            crtime,
            uid,
            gid,
            first_cluster: dir_entry.first_cluster(),
        }
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
}

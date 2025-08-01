use std::ffi::c_int;
use std::io::{Read, Write};
use std::rc::Rc;
use std::time::Duration;

use fat_bits::dir::DirEntry;
use fuser::{FileType, Filesystem};
use libc::{EBADF, EINVAL, EIO, EISDIR, ENOENT, ENOSYS, ENOTDIR};
use log::{debug, error};

use crate::FatFuse;

const TTL: Duration = Duration::from_secs(1);

impl Filesystem for FatFuse {
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), c_int> {
        Ok(())
    }

    fn destroy(&mut self) {
        debug!("inode_table: {}", self.inode_table.len());

        debug!("ino_by_first_cluster: {}", self.ino_by_first_cluster.len());
        for (&first_cluster, &ino) in self.ino_by_first_cluster.iter() {
            debug!("{} -> {}", first_cluster, ino);
        }

        debug!("ino_by_fh: {}", self.ino_by_fh.len());

        debug!("ino_by_path: {}", self.ino_by_path.len());
    }

    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        let Some(name) = name.to_str() else {
            // TODO: add proper handling of non-utf8 strings
            debug!("cannot convert OsStr {:?} to str", name);
            reply.error(ENOSYS);
            return;
        };

        debug!("looking up file {} with parent ino {}", name, parent);

        let Some(parent_inode) = self.get_inode(parent).cloned() else {
            // parent inode does not exist
            // TODO: how can we make sure this does not happed?
            // TODO: panic?
            debug!("could not find inode for parent ino {}", parent);

            reply.error(ENOENT);
            return;
        };

        let parent_inode = parent_inode.borrow();

        let dir_entry: DirEntry =
            match parent_inode
                .dir_iter(&self.fat_fs)
                .and_then(|mut dir_iter| {
                    dir_iter
                        .find(|dir_entry| &dir_entry.name_string() == name)
                        .ok_or(ENOENT)
                }) {
                Ok(dir_entry) => dir_entry,
                Err(err) => {
                    debug!("error: {}", err);
                    reply.error(err);

                    return;
                }
            };

        let inode = self.get_or_make_inode_by_dir_entry(
            &dir_entry,
            parent_inode.ino(),
            parent_inode.path(),
        );

        let mut inode = inode.borrow_mut();

        let attr = inode.file_attr();
        let generation = inode.generation();

        reply.entry(&TTL, &attr, generation as u64);

        inode.inc_ref_count();
    }

    fn forget(&mut self, _req: &fuser::Request<'_>, ino: u64, nlookup: u64) {
        debug!("forgetting ino {} ({} times)", ino, nlookup);

        let Some(inode) = self.get_inode(ino).cloned() else {
            debug!("tried to forget {} refs of inode {}, but was not found", ino, nlookup);

            return;
        };

        let mut inode_ref = inode.borrow_mut();

        if inode_ref.dec_ref_count(nlookup) == 0 {
            debug!("dropping inode {}", inode_ref.ino());

            drop(inode_ref);

            // no more references, drop inode
            self.drop_inode(inode);
        }
    }

    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: Option<u64>,
        reply: fuser::ReplyAttr,
    ) {
        // warn!("[Not Implemented] getattr(ino: {:#x?}, fh: {:#x?})", ino, fh);
        // reply.error(ENOSYS);

        let inode = if let Some(fh) = fh {
            let Some(inode) = self.get_inode_by_fh(fh) else {
                reply.error(EBADF);
                return;
            };

            inode
        } else if let Some(inode) = self.get_inode(ino).cloned() {
            inode
        } else {
            reply.error(ENOENT);
            return;
        };

        let inode = inode.borrow();

        let attr = inode.file_attr();

        reply.attr(&TTL, &attr);
    }

    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        debug!(
            "[Not Implemented] setattr(ino: {:#x?}, mode: {:?}, uid: {:?}, \
            gid: {:?}, size: {:?}, fh: {:?}, flags: {:?})",
            ino, mode, uid, gid, size, fh, flags
        );
        reply.error(ENOSYS);
        return;

        // TODO: implement this properly
        // let Some(inode) = self.get_inode(ino) else {
        // debug!("tried to get inode {ino}, but not found");
        //
        // reply.error(ENOENT);
        // return;
        // };
        //
        // reply.attr(&TTL, &inode.file_attr());
    }

    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!("[Not Implemented] readlink(ino: {:#x?})", ino);
        reply.error(ENOSYS);
    }

    fn mknod(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] mknod(parent: {:#x?}, name: {:?}, mode: {}, \
            umask: {:#x?}, rdev: {})",
            parent, name, mode, umask, rdev
        );
        reply.error(ENOSYS);
    }

    fn mkdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] mkdir(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?})",
            parent, name, mode, umask
        );
        reply.error(ENOSYS);
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("[Not Implemented] unlink(parent: {:#x?}, name: {:?})", parent, name,);
        reply.error(ENOSYS);
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("[Not Implemented] rmdir(parent: {:#x?}, name: {:?})", parent, name,);
        reply.error(ENOSYS);
    }

    fn rename(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        newparent: u64,
        newname: &std::ffi::OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] rename(parent: {:#x?}, name: {:?}, newparent: {:#x?}, \
            newname: {:?}, flags: {})",
            parent, name, newparent, newname, flags,
        );
        reply.error(ENOSYS);
    }

    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        if !self.inode_table.contains_key(&ino) {
            reply.error(ENOENT);
            return;
        }

        let fh = self.next_fh();

        if let Some(old_ino) = self.ino_by_fh.insert(fh, ino) {
            debug!("fh {} was associated with ino {}, now with ino {}", fh, old_ino, ino);
        }

        debug!("opened inode {}: fh {}", ino, fh);

        reply.opened(fh, 0);
    }

    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        // warn!(
        //     "[Not Implemented] read(ino: {:#x?}, fh: {}, offset: {}, size: {}, \
        //     flags: {:#x?}, lock_owner: {:?})",
        //     ino, fh, offset, size, flags, lock_owner
        // );
        // reply.error(ENOSYS);

        if offset < 0 {
            debug!("tried to read with negative offset {offset}");

            reply.error(EINVAL);
            return;
        }

        let offset = offset as u64;

        let Some(inode) = self.get_inode_by_fh(fh) else {
            debug!("fh {fh} is not associated by any inode");

            reply.error(EBADF);
            return;
        };

        let inode = inode.borrow();

        if inode.ino() != ino {
            debug!("fh {fh} is associated with inode {} instead of {ino}", inode.ino());

            reply.error(EINVAL);
            return;
        }

        if !inode.is_file() {
            debug!("tried to use read on directory {ino}");

            reply.error(EISDIR);
            return;
        }

        let mut reader = match inode.file_reader(&self.fat_fs) {
            Ok(reader) => reader,
            Err(err) => {
                reply.error(err);
                return;
            }
        };

        if reader.skip(offset) != offset {
            // reader is exhausted, bail
            reply.data(&[]);
            return;
        }

        let mut buf = vec![0; size as usize];

        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(err) => {
                error!("error while reading: {err}");

                reply.error(EIO);
                return;
            }
        };

        debug!("read {n} bytes");

        reply.data(&buf[..n]);
    }

    fn write(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        debug!("new write request: ino={ino} fh={fh} offset={offset} data={data:?}");

        if offset < 0 {
            debug!("tried to write with negative offset {offset}");

            reply.error(EINVAL);
            return;
        }

        let offset = offset as u64;

        let Some(inode) = self.get_inode_by_fh(fh) else {
            debug!("no inode associated with fh {fh} (given ino: {ino}");

            reply.error(EBADF);
            return;
        };

        let inode = inode.borrow();

        if inode.is_read_only() {
            reply.error(EBADF);
            return;
        }

        if inode.ino() != ino {
            debug!("fh {fh} points to ino {}, but ino {ino} was given", inode.ino());

            reply.error(EINVAL);
            return;
        }

        if !inode.is_file() {
            debug!("tried to use read on directory {ino}");

            reply.error(EISDIR);
            return;
        }

        let mut writer = match inode.file_writer(&self.fat_fs) {
            Ok(writer) => writer,
            Err(err) => {
                reply.error(err);
                return;
            }
        };

        // if writer.skip(offset) != offset {
        //     // writer is at EOF, bail

        // }

        let cur_offset = writer.skip(offset);

        // can't seek more than we requested
        assert!(cur_offset <= offset);

        let mut bytes_written = 0;

        if cur_offset < offset {
            // tried to set offset beyond EOF
            // fill with zeros
            let zeros = vec![0; (offset - cur_offset) as usize];

            debug!("writing {} zeros", zeros.len());

            if let Err(err) = writer.write_all(&zeros) {
                debug!("writing zeros returned error: {err}");

                reply.error(EIO);
                return;
            }

            bytes_written += zeros.len();
        }

        if let Err(err) = writer.write_all(&data) {
            debug!("writing data returned error: {err}");

            reply.error(EIO);
            return;
        }

        bytes_written += data.len();

        reply.written(bytes_written as u32);

        // TODO: update file size
        if offset + bytes_written as u64 > inode.size() {
            todo!()
        }
    }

    fn flush(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        // debug!(
        //     "[Not Implemented] flush(ino: {:#x?}, fh: {}, lock_owner: {:?})",
        //     ino, fh, lock_owner
        // );
        // reply.error(ENOSYS);

        debug!("flushing ino={ino} fh={fh}");

        let Some(&found_ino) = self.ino_by_fh.get(&fh) else {
            debug!("expected fh {fh} to be mapped to ino {ino}, but not found instead");

            reply.error(EBADF);
            return;
        };

        if found_ino != ino {
            debug!(
                "expected fh {fh} to be mapped to ino {ino}, but was mapped to {found_ino} instead"
            );

            reply.error(EBADF);
            return;
        }

        reply.ok();
    }

    fn release(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        let Some(found_ino) = self.ino_by_fh.remove(&fh) else {
            debug!("tried to release fh {fh} with ino {ino}, but no ino was found in mapping");

            reply.error(EBADF);
            return;
        };

        if found_ino != ino {
            debug!("tried to release fh {fh} with ino {ino}, but found ino is {found_ino} instead");

            reply.error(EINVAL);
            return;
        }

        reply.ok();
    }

    fn fsync(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("[Not Implemented] fsync(ino: {:#x?}, fh: {}, datasync: {})", ino, fh, datasync);
        reply.error(ENOSYS);
    }

    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        let fh = self.next_fh();

        if let Some(old_ino) = self.ino_by_fh.insert(fh, ino) {
            debug!("fh {} was already associated with ino {}, now with ino {}", fh, old_ino, ino);
        }

        reply.opened(fh, 0);
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        let Ok(mut offset): Result<usize, _> = offset.try_into() else {
            return;
        };

        let Some(dir_inode) = self.get_inode_by_fh(fh) else {
            debug!("could not find inode accociated with fh {} (ino: {})", fh, ino);

            reply.error(EBADF);
            return;
        };

        let dir_inode = dir_inode.borrow();

        if dir_inode.ino() != ino {
            debug!(
                "ino {} of inode associated with fh {} does not match given ino {}",
                dir_inode.ino(),
                fh,
                ino
            );

            reply.error(EINVAL);
            return;
        }

        let mut _next_idx = 1;
        let mut next_offset = || {
            let next_idx = _next_idx;
            _next_idx += 1;
            next_idx
        };

        if dir_inode.is_root() {
            if offset == 0 {
                debug!("adding . to root dir");
                if reply.add(1, next_offset(), FileType::Directory, ".") {
                    return;
                }
            } else {
                offset -= 1;
            }

            if offset == 0 {
                debug!("adding .. to root dir");
                if reply.add(1, next_offset(), FileType::Directory, "..") {
                    return;
                }
            } else {
                offset -= 1;
            }
        }

        let Ok(dir_iter) = dir_inode.dir_iter(&self.fat_fs) else {
            reply.error(ENOTDIR);
            return;
        };

        // need to drop dir_iter here so we can borrow self mut again
        // also skip over `offset` entries
        let dirs: Vec<DirEntry> = dir_iter.skip(offset).collect();

        let dir_ino = dir_inode.ino();
        let dir_path = dir_inode.path();

        for dir_entry in dirs {
            let name = dir_entry.name_string();

            let inode =
                self.get_or_make_inode_by_dir_entry(&dir_entry, dir_ino, Rc::clone(&dir_path));

            let inode = inode.borrow();

            debug!("adding entry {} (ino: {})", name, inode.ino());
            if reply.add(ino, next_offset(), inode.kind().into(), name) {
                return;
            }
        }

        reply.ok();
    }

    fn readdirplus(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectoryPlus,
    ) {
        debug!(
            "[Not Implemented] readdirplus(ino: {:#x?}, fh: {}, offset: {})",
            ino, fh, offset
        );
        reply.error(ENOSYS);
    }

    fn releasedir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        _flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        let Some(ino) = self.ino_by_fh.remove(&fh) else {
            debug!("can't find inode {} by fh {}", ino, fh);

            reply.error(EBADF);
            return;
        };

        let Some(inode) = self.get_inode(ino) else {
            debug!("ino {} not associated with an inode", ino);

            reply.ok();
            return;
        };

        let inode = inode.borrow();

        if inode.ino() != ino {
            debug!(
                "inode with ino {}, associated with fh {}, does not have expected ino {}",
                inode.ino(),
                fh,
                ino
            );
        }

        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fsyncdir(ino: {:#x?}, fh: {}, datasync: {})",
            ino, fh, datasync
        );
        reply.error(ENOSYS);
    }

    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }

    fn create(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        debug!(
            "[Not Implemented] create(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?}, \
            flags: {:#x?})",
            parent, name, mode, umask, flags
        );
        reply.error(ENOSYS);
    }

    fn lseek(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: fuser::ReplyLseek,
    ) {
        debug!(
            "[Not Implemented] lseek(ino: {:#x?}, fh: {}, offset: {}, whence: {})",
            ino, fh, offset, whence
        );
        reply.error(ENOSYS);
    }
}

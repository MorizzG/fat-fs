use std::fmt::Debug;
use std::io::{Read, Write};

use crate::{FatFs, SliceLike};

pub struct SubSliceMut<'a, S: SliceLike> {
    fat_fs: &'a mut FatFs<S>,

    offset: u64,
    len: usize,
}

impl<S: SliceLike> Debug for SubSliceMut<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl<S: SliceLike> SubSliceMut<'_, S> {
    pub fn new(fat_fs: &mut FatFs<S>, offset: u64, len: usize) -> SubSliceMut<'_, S> {
        SubSliceMut {
            fat_fs,
            offset,
            len,
        }
    }
}

impl<S: SliceLike> SubSliceMut<'_, S> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<S: SliceLike> Read for SubSliceMut<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_to_read = self.len.min(buf.len());

        self.fat_fs
            .inner
            .borrow_mut()
            .read_at_offset(self.offset, &mut buf[..bytes_to_read])?;

        self.offset += bytes_to_read as u64;
        self.len -= bytes_to_read;

        Ok(bytes_to_read)
    }
}

impl<S: SliceLike> Write for SubSliceMut<'_, S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_to_write = self.len.min(buf.len());

        self.fat_fs
            .inner
            .borrow_mut()
            .write_at_offset(self.offset, &buf[..bytes_to_write])?;

        self.offset += bytes_to_write as u64;
        self.len -= bytes_to_write;

        Ok(bytes_to_write)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct SubSlice<'a, S: SliceLike> {
    fat_fs: &'a FatFs<S>,

    offset: u64,
    len: usize,
}

impl<S: SliceLike> Debug for SubSlice<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl<S: SliceLike> SubSlice<'_, S> {
    pub fn new(fat_fs: &FatFs<S>, offset: u64, len: usize) -> SubSlice<'_, S> {
        SubSlice {
            fat_fs,
            offset,
            len,
        }
    }

    pub fn fat_fs(&self) -> &FatFs<S> {
        self.fat_fs
    }

    pub fn fat_fs_mut(&self) -> &FatFs<S> {
        self.fat_fs
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
}

impl<'a, S: SliceLike> SubSlice<'a, S> {
    /// releases the inner &FatFs, consuming self in the process
    pub fn release(self) -> &'a FatFs<S> {
        self.fat_fs
    }
}

impl<S: SliceLike> Read for SubSlice<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_to_read = self.len.min(buf.len());

        self.fat_fs
            .inner
            .borrow_mut()
            .read_at_offset(self.offset, &mut buf[..bytes_to_read])?;

        self.offset += bytes_to_read as u64;
        self.len -= bytes_to_read;

        Ok(bytes_to_read)
    }
}

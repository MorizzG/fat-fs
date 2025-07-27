use std::fmt::Debug;
use std::io::{Read, Write};

use crate::FatFs;

pub struct SubSliceMut<'a> {
    fat_fs: &'a mut FatFs,

    offset: u64,
    len: usize,
}

impl Debug for SubSliceMut<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl SubSliceMut<'_> {
    pub fn new(fat_fs: &mut FatFs, offset: u64, len: usize) -> SubSliceMut<'_> {
        SubSliceMut {
            fat_fs,
            offset,
            len,
        }
    }
}

impl SubSliceMut<'_> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Read for SubSliceMut<'_> {
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

impl Write for SubSliceMut<'_> {
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

pub struct SubSlice<'a> {
    fat_fs: &'a FatFs,

    offset: u64,
    len: usize,
}

impl Debug for SubSlice<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl SubSlice<'_> {
    pub fn new(fat_fs: &FatFs, offset: u64, len: usize) -> SubSlice<'_> {
        SubSlice {
            fat_fs,
            offset,
            len,
        }
    }

    pub fn fat_fs(&self) -> &FatFs {
        self.fat_fs
    }

    pub fn fat_fs_mut(&self) -> &FatFs {
        self.fat_fs
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
}

impl<'a> SubSlice<'a> {
    /// releases the inner &FatFs, consuming self in the process
    pub fn release(self) -> &'a FatFs {
        self.fat_fs
    }
}

impl Read for SubSlice<'_> {
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

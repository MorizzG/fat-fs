use std::cell::RefCell;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::rc::Rc;

use crate::SliceLike;

pub struct SubSliceMut {
    // fat_fs: &'a FatFs,
    data: Rc<RefCell<dyn SliceLike>>,

    offset: u64,
    len: usize,
}

impl Debug for SubSliceMut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl SubSliceMut {
    pub fn new(data: Rc<RefCell<dyn SliceLike>>, offset: u64, len: usize) -> SubSliceMut {
        SubSliceMut { data, offset, len }
    }
}

impl<'a> SubSliceMut {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn skip(&mut self, n: usize) -> usize {
        let n = n.min(self.len());

        self.offset += n as u64;
        self.len -= n;

        n
    }
}

impl Read for SubSliceMut {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_to_read = self.len.min(buf.len());

        self.data
            .borrow_mut()
            .read_at_offset(self.offset, &mut buf[..bytes_to_read])?;

        self.offset += bytes_to_read as u64;
        self.len -= bytes_to_read;

        Ok(bytes_to_read)
    }
}

impl Write for SubSliceMut {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_to_write = self.len.min(buf.len());

        self.data
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

pub struct SubSlice {
    data: Rc<RefCell<dyn SliceLike>>,

    offset: u64,
    len: usize,
}

impl Debug for SubSlice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubSliceMut")
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl<'a> SubSlice {
    pub fn new(data: Rc<RefCell<dyn SliceLike>>, offset: u64, len: usize) -> SubSlice {
        SubSlice { data, offset, len }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn skip(&mut self, n: usize) -> usize {
        let n = n.min(self.len());

        self.offset += n as u64;
        self.len -= n;

        n
    }
}

impl Read for SubSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_to_read = self.len.min(buf.len());

        self.data
            .borrow_mut()
            .read_at_offset(self.offset, &mut buf[..bytes_to_read])?;

        self.offset += bytes_to_read as u64;
        self.len -= bytes_to_read;

        Ok(bytes_to_read)
    }
}

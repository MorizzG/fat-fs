use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};

pub trait SliceLike {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()>;
}

impl SliceLike for &mut [u8] {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        if offset as usize + buf.len() > self.len() {
            return Err(std::io::Error::other(anyhow::anyhow!(
                "reading {} bytes at offset {} is out of bounds for slice of len {}",
                buf.len(),
                offset,
                self.len()
            )));
        }

        buf.copy_from_slice(&self[offset as usize..][..buf.len()]);

        Ok(())
    }

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        if offset as usize + bytes.len() > self.len() {
            return Err(std::io::Error::other(anyhow::anyhow!(
                "writing {} bytes at offset {} is out of bounds for slice of len {}",
                bytes.len(),
                offset,
                self.len()
            )));
        }

        self[offset as usize..][..bytes.len()].copy_from_slice(bytes);

        Ok(())
    }
}

impl SliceLike for File {
    fn read_at_offset(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;

        self.read_exact(buf)?;

        Ok(())
    }

    fn write_at_offset(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;

        self.write_all(bytes)?;

        Ok(())
    }
}

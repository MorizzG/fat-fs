use std::fmt::Display;
use std::io::Read;

use bitflags::bitflags;
use chrono::{NaiveDate, NaiveDateTime, TimeDelta};

use crate::datetime::{Date, Time};
use crate::dir;
use crate::utils::{load_u16_le, load_u32_le};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Attr: u8 {
        const ReadOnly = 0x01;
        const Hidden = 0x02;
        const System = 0x04;
        const VolumeId = 0x08;
        const Directory = 0x10;
        const Archive = 0x20;

        // const _ = !0;

        // ReadOnly + Hidden + System + Volumeid
        const LongName = 0x01 | 0x02 | 0x04 | 0x08;
    }
}

impl Display for Attr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut if_has_attr = |attr: Attr, c: char| {
            if self.contains(attr) {
                write!(f, "{}", c)
            } else {
                write!(f, "-")
            }
        };

        if_has_attr(Attr::ReadOnly, 'R')?;
        if_has_attr(Attr::Hidden, 'H')?;
        if_has_attr(Attr::System, 'S')?;
        if_has_attr(Attr::VolumeId, 'V')?;
        if_has_attr(Attr::Directory, 'D')?;
        if_has_attr(Attr::Archive, 'A')?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct RegularDirEntry {
    name: [u8; 11],
    attr: Attr,

    create_time_tenths: u8,
    create_time: Time,
    create_date: Date,

    last_access_date: Date,

    first_cluster: u32,

    write_time: Time,
    write_date: Date,

    file_size: u32,

    long_name: Option<String>,
}

impl Display for RegularDirEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut name = self.name_string().unwrap_or_else(|| "<unknown>".to_owned());

        if self.attr.contains(Attr::Directory) {
            name.push('/');
        }

        write!(
            f,
            "{}    {}",
            self.attr,
            // self.create_time().format("%a %b %d %H:%M:%S%.3f %Y"),
            // self.write_time().format("%a %b %d %H:%M:%S%.3f %Y"),
            name,
        )?;

        Ok(())
    }
}

impl RegularDirEntry {
    pub fn load(bytes: &[u8]) -> anyhow::Result<RegularDirEntry> {
        assert_eq!(bytes.len(), 32);

        let name = bytes[..11].try_into().unwrap();
        let attr = Attr::from_bits_truncate(bytes[11]);

        let create_time_tenths = bytes[13];
        anyhow::ensure!(
            create_time_tenths <= 199,
            "invalid DIR_CrtTimeTenth: {}",
            create_time_tenths
        );

        let create_time = Time::new(load_u16_le(&bytes[14..][..2]))?;
        let create_date = Date::new(load_u16_le(&bytes[16..][..2]))?;
        let last_access_date = Date::new(load_u16_le(&bytes[18..][..2]))?;
        let write_time = Time::new(load_u16_le(&bytes[22..][..2]))?;
        let write_date = Date::new(load_u16_le(&bytes[24..][..2]))?;
        let file_size = load_u32_le(&bytes[28..][..4]);

        let first_cluster_hi = load_u16_le(&bytes[20..][..2]);
        let first_cluster_lo = load_u16_le(&bytes[26..][..2]);

        let first_cluster = first_cluster_lo as u32 | ((first_cluster_hi as u32) << 16);

        if attr.contains(Attr::VolumeId) {
            anyhow::ensure!(
                first_cluster == 0,
                "DirEntry has volume id attribute set, but first cluster is {}, not zero",
                first_cluster
            );
        }

        if attr.contains(Attr::Directory) {
            anyhow::ensure!(
                file_size == 0,
                "DirEntry has directory attribute set, but file size is {}, not zero",
                file_size
            )
        }

        Ok(RegularDirEntry {
            name,
            attr,
            create_time_tenths,
            create_time,
            create_date,
            last_access_date,
            first_cluster,
            write_time,
            write_date,
            file_size,
            long_name: None,
        })
    }

    /// indicates this DirEntry is empty
    ///
    /// can be either simply empty (0xe5) or the sentinel (0x00) that indicates that all following
    /// DirEntries are empty as well
    pub fn is_empty(&self) -> bool {
        self.name[0] == 0xe5 || self.name[0] == 0x00
    }

    /// indicates this and all following DisEntries are empty
    pub fn is_sentinel(&self) -> bool {
        self.name[0] == 0x00
    }

    pub fn is_file(&self) -> bool {
        !self
            .attr
            .intersects(Attr::Directory | Attr::System | Attr::VolumeId)
    }

    pub fn is_dir(&self) -> bool {
        self.attr.contains(Attr::Directory) && !self.attr.intersects(Attr::System | Attr::VolumeId)
    }

    pub fn is_dot(&self) -> bool {
        if !self.is_dir() {
            return false;
        }

        // &self.name[..2] == &[b'.', b' ']

        self.name[0] == b'.' && &self.name[1..] == &[b' '; 10]
    }

    pub fn is_dotdot(&self) -> bool {
        if !self.is_dir() {
            return false;
        }

        // &self.name[..3] == &[b'.', b'.', b' ']

        &self.name[..2] == &[b'.', b'.'] && &self.name[2..] == &[b' '; 9]
    }

    pub fn is_hidden(&self) -> bool {
        self.is_dot() || self.is_dotdot() || self.attr.contains(Attr::Hidden)
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn name_string(&self) -> Option<String> {
        if let Some(long_filename) = self.long_name() {
            return Some(long_filename.to_owned());
        }

        let name = std::str::from_utf8(&self.name[..8]).ok()?.trim_ascii_end();
        let ext = std::str::from_utf8(&self.name[8..]).ok()?.trim_ascii_end();

        let mut s = String::new();

        if self.attr.contains(Attr::Hidden) {
            s.push('.');
        }

        s += name;

        if !ext.is_empty() {
            s.push('.');

            s += ext;
        }

        Some(s)
    }

    pub fn long_name(&self) -> Option<&str> {
        self.long_name.as_deref()
    }

    pub fn set_long_name(&mut self, long_name: String) {
        self.long_name = Some(long_name);
    }

    pub fn attr(&self) -> Attr {
        self.attr
    }

    pub fn create_time(&self) -> NaiveDateTime {
        let date = self.create_date.to_naive_date();
        let time = self.create_time.to_naive_time();

        let time_frac = TimeDelta::try_milliseconds(100 * self.create_time_tenths as i64).unwrap();

        let time = time.overflowing_add_signed(time_frac).0;

        NaiveDateTime::new(date, time)
    }

    pub fn last_access_date(&self) -> NaiveDate {
        self.last_access_date.to_naive_date()
    }

    pub fn first_cluster(&self) -> u32 {
        self.first_cluster
    }

    pub fn write_time(&self) -> NaiveDateTime {
        let time = self.write_time.to_naive_time();
        let date = self.write_date.to_naive_date();

        NaiveDateTime::new(date, time)
    }

    pub fn file_size(&self) -> u32 {
        self.file_size
    }

    pub fn checksum(&self) -> u8 {
        let mut checksum: u8 = 0;

        for &x in self.name() {
            checksum = checksum.rotate_right(1).wrapping_add(x);
        }

        checksum
    }
}

pub struct LongNameDirEntry {
    ordinal: u8,
    is_last: bool,
    name: [u16; 13],
    checksum: u8,
}

impl LongNameDirEntry {
    pub fn load(bytes: &[u8]) -> anyhow::Result<LongNameDirEntry> {
        assert_eq!(bytes.len(), 32);

        let ordinal = bytes[0] & !0x40;
        let is_last = (bytes[0] & 0x40) != 0;

        let name1 = &bytes[1..][..10];

        let attr = Attr::from_bits_retain(bytes[11]);

        anyhow::ensure!(attr.contains(Attr::LongName), "not a long name entry");
        anyhow::ensure!(bytes[12] == 0, "LDIR_Type must be 0, not {}", bytes[12]);

        let checksum = bytes[13];

        let name2 = &bytes[14..][..12];

        anyhow::ensure!(
            &bytes[26..][..2] == &[0, 0],
            "LDIR_FstClusLO must be zero, not 0x{:04X}",
            load_u32_le(&bytes[26..][..2])
        );

        let name3 = &bytes[28..][..4];

        let mut name = [0; 13];

        for (x, y) in name1
            .chunks_exact(2)
            .chain(name2.chunks_exact(2))
            .chain(name3.chunks_exact(2))
            .map(|x| load_u16_le(x))
            .zip(name.iter_mut())
        {
            *y = x;
        }

        Ok(LongNameDirEntry {
            ordinal,
            is_last,
            name,
            checksum,
        })
    }

    pub fn ordinal(&self) -> u8 {
        self.ordinal
    }

    pub fn is_first(&self) -> bool {
        self.is_last
    }

    pub fn name(&self) -> &[u16] {
        &self.name
    }

    pub fn checksum(&self) -> u8 {
        self.checksum
    }
}

pub enum DirEntry {
    Regular(RegularDirEntry),
    LongName(LongNameDirEntry),
}

impl DirEntry {
    pub fn load(bytes: &[u8]) -> anyhow::Result<DirEntry> {
        assert_eq!(bytes.len(), 32);

        let attr = Attr::from_bits_truncate(bytes[11]);

        let dir_entry = if attr == Attr::LongName {
            DirEntry::LongName(LongNameDirEntry::load(bytes)?)
        } else {
            DirEntry::Regular(RegularDirEntry::load(bytes)?)
        };

        Ok(dir_entry)
    }
}

#[derive(Debug, Default)]
struct LongFilenameBuf {
    rev_buf: Vec<u16>,
    checksum: Option<u8>,
    last_ordinal: Option<u8>,
}

impl LongFilenameBuf {
    pub fn reset(&mut self) {
        self.rev_buf.clear();
        self.checksum = None;
        self.last_ordinal = None;
    }

    pub fn next(&mut self, dir_entry: LongNameDirEntry) -> anyhow::Result<()> {
        if dir_entry.is_last {
            // first/lasts entry

            let mut name = dir_entry.name();

            while name.last() == Some(&0xFFFF) {
                name = &name[..name.len() - 1];
            }

            assert!(!name.is_empty());

            self.extend_name(name);
            self.checksum = Some(dir_entry.checksum());
            self.last_ordinal = Some(dir_entry.ordinal());

            return Ok(());
        }

        assert!(self.checksum.is_some());

        anyhow::ensure!(
            self.checksum == Some(dir_entry.checksum()),
            "checksum doesn't match previous"
        );

        anyhow::ensure!(
            self.last_ordinal.unwrap() != 1,
            "last ordinal was 1, but found more entries"
        );
        anyhow::ensure!(
            self.last_ordinal.unwrap() - 1 == dir_entry.ordinal,
            "expected ordinal {}, but found {} instead",
            self.last_ordinal.unwrap() - 1,
            dir_entry.ordinal()
        );

        self.extend_name(dir_entry.name());
        self.last_ordinal = Some(dir_entry.ordinal());

        Ok(())
    }

    fn extend_name(&mut self, name: &[u16]) {
        self.rev_buf.extend(name.iter().rev());
    }

    pub fn get_buf(&mut self, checksum: u8) -> anyhow::Result<Option<impl Iterator<Item = u16>>> {
        if self.checksum.is_none() {
            return Ok(None);
        }

        anyhow::ensure!(
            self.last_ordinal.is_some() && self.checksum.is_some(),
            "long filename buffer is empty"
        );

        anyhow::ensure!(
            self.last_ordinal.unwrap() == 1,
            "last ordinal is {}, not 1",
            self.last_ordinal.unwrap()
        );
        anyhow::ensure!(
            self.checksum.unwrap() == checksum,
            "given checksum 0x{:02X} doesn't match previous checksum 0x{:02X}",
            checksum,
            self.checksum.unwrap()
        );

        Ok(Some(self.rev_buf.iter().copied().rev()))
    }
}

pub struct DirIter<R: Read> {
    reader: R,

    // long_filename_rev_buf: Vec<u16>,
    // long_filename_checksum: Option<u8>,
    // long_filename_last_ordinal: Option<u8>,
    long_filename_buf: LongFilenameBuf,
}

impl<R: Read> DirIter<R> {
    pub fn new(reader: R) -> DirIter<R> {
        DirIter {
            reader,
            // long_filename_rev_buf: Vec::new(),
            // long_filename_checksum: None,
            // long_filename_last_ordinal: None,
            long_filename_buf: Default::default(),
        }
    }
}

impl<R: Read> Iterator for DirIter<R> {
    type Item = RegularDirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let mut chunk = [0; 32];
        self.reader.read_exact(&mut chunk).ok()?;

        // let Ok(dir_entry) = DirEntry::load(&chunk) else {
        //     return self.next();
        // };

        let dir_entry = match DirEntry::load(&chunk) {
            Ok(dir_entry) => dir_entry,
            Err(e) => {
                // if loading fails: print error and try next entry
                eprintln!("failed to load dir entry: {e}");

                return self.next();
            }
        };

        let mut dir_entry = match dir_entry {
            DirEntry::Regular(dir_entry) => dir_entry,
            DirEntry::LongName(long_name) => {
                if let Err(e) = self.long_filename_buf.next(long_name) {
                    eprintln!("invalid long filename entry: {}", e);
                }

                // simply skip long name entries for now
                return self.next();
            }
        };

        if dir_entry.is_sentinel() {
            return None;
        }

        if dir_entry.is_empty() {
            return self.next();
        }

        match self.long_filename_buf.get_buf(dir_entry.checksum()) {
            Ok(Some(iter)) => {
                // attach long filename to dir_entry

                let long_filename: String =
                    char::decode_utf16(iter).filter_map(|x| x.ok()).collect();

                dir_entry.set_long_name(long_filename);
            }
            Ok(None) => {} // no long filename -> do nothing
            Err(e) => {
                eprintln!(
                    "failed to get long filename for {}: {}",
                    dir_entry.name_string().as_deref().unwrap_or("<invalid>"),
                    e
                );
            }
        }

        self.long_filename_buf.reset();

        Some(dir_entry)
    }
}

use std::fmt::Display;
use std::io::Read;

use bitflags::bitflags;
use chrono::{NaiveDate, NaiveDateTime, TimeDelta};

use crate::datetime::{Date, Time};
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
            "DirEntry {{ {: <16}    created: {}    modified: {} }}",
            name,
            self.create_time(),
            self.write_time()
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

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn name_string(&self) -> Option<String> {
        // std::str::from_utf8(self.name()).ok()

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
}

pub struct LongNameDirEntry {}

impl LongNameDirEntry {
    pub fn load(bytes: &[u8]) -> anyhow::Result<LongNameDirEntry> {
        assert_eq!(bytes.len(), 32);

        Ok(LongNameDirEntry {})
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

pub struct DirIter<R: Read> {
    reader: R,
}

impl<R: Read> DirIter<R> {
    pub fn new(reader: R) -> DirIter<R> {
        DirIter { reader }
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

        let DirEntry::Regular(dir_entry) = dir_entry else {
            // simply skip long name entries for now
            return self.next();
        };

        if dir_entry.is_sentinel() {
            return None;
        }

        if dir_entry.is_empty() {
            return self.next();
        }

        Some(dir_entry)
    }
}

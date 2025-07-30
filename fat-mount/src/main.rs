use std::cell::RefCell;
use std::fs::File;
use std::rc::Rc;

use fat_fuse::FatFuse;
use fuser::MountOption;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut args = std::env::args();

    let path = args.next().ok_or(anyhow::anyhow!("missing fs path"))?;
    let mountpoint = args.next().ok_or(anyhow::anyhow!("missing mount point"))?;

    let file = File::open(path)?;

    let fat_fuse = FatFuse::new(Rc::new(RefCell::new(file)))?;

    let options = vec![
        MountOption::RO,
        MountOption::FSName("fat-fuse".to_owned()),
        MountOption::AutoUnmount,
    ];

    fuser::mount2(fat_fuse, mountpoint, &options).unwrap();

    Ok(())
}

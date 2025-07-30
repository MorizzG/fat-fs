use std::fs::File;
use std::sync::mpsc::channel;

use fat_fuse::FatFuse;
use fuser::MountOption;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut args = std::env::args();

    let _prog_name = args.next().unwrap();
    let path = args.next().ok_or(anyhow::anyhow!("missing fs path"))?;
    let mountpoint = args.next().ok_or(anyhow::anyhow!("missing mount point"))?;

    let file = File::open(path)?;

    let fat_fuse = FatFuse::new(file)?;

    let options = vec![
        MountOption::RO,
        MountOption::FSName("fat-fuse".to_owned()),
        MountOption::AutoUnmount,
    ];

    let (tx, rx) = channel();

    ctrlc::set_handler(move || {
        tx.send(()).unwrap();
    })
    .unwrap();

    let handle = fuser::spawn_mount2(fat_fuse, mountpoint, &options)?;

    rx.recv().unwrap();

    println!("done");

    Ok(())
}

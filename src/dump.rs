use std::io::Read;

use fat_rs::FatFs;
use fat_rs::fat::Fatty as _;

pub fn main() -> anyhow::Result<()> {
    let args = std::env::args();

    if args.len() != 2 {
        anyhow::bail!("usage: dump <path>");
    }

    let file = std::fs::File::open(args.skip(1).next().unwrap())?;

    // let mut buf = [0; 512];

    // file.read_exact(&mut buf)?;

    // let bpb = Bpb::load(&buf)?;

    // println!("{}", bpb);

    let mut fat_fs = FatFs::load(file)?;

    println!("{}", fat_fs.bpb());
    println!();
    println!("{}", fat_fs.fat());
    println!();
    println!(
        "free clusters: {} ({} bytes)",
        fat_fs.fat().count_free_clusters(),
        fat_fs.fat().count_free_clusters()
            * fat_fs.bpb().bytes_per_sector() as usize
            * fat_fs.bpb().sectors_per_cluster() as usize
    );

    for dir_entry in fat_fs.root_dir_iter() {
        println!("{}", dir_entry);
    }

    Ok(())
}

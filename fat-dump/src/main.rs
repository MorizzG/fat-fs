use fat_bits::FatFs;
use fat_bits::dir::DirEntry;

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

    let fat_fs = FatFs::load(file)?;

    // println!("{}", fat_fs.bpb());
    // println!();
    // println!("{}", fat_fs.fat());

    println!("{}", fat_fs);
    println!();
    println!(
        "free clusters: {} ({} bytes)",
        fat_fs.free_clusters(),
        fat_fs.free_clusters() as usize
            * fat_fs.bytes_per_sector() as usize
            * fat_fs.sectors_per_cluster() as usize
    );

    println!();
    println!();

    tree(&fat_fs, false);

    Ok(())
}

fn tree(fat_fs: &FatFs, show_hidden: bool) {
    fn do_indent(indent: u32) {
        for _ in 0..indent {
            print!("    ");
        }
    }

    fn tree_impl(
        fat_fs: &FatFs,
        iter: impl Iterator<Item = DirEntry>,
        show_hidden: bool,
        indent: u32,
    ) {
        for dir_entry in iter.filter(|x| show_hidden || !x.is_hidden()) {
            do_indent(indent);

            println!("{}", dir_entry);

            if dir_entry.is_dot() || dir_entry.is_dotdot() {
                // do not descent into . and ..
                continue;
            }

            if dir_entry.is_dir() {
                let iter = fat_fs.dir_iter(dir_entry.first_cluster());

                tree_impl(fat_fs, iter, show_hidden, indent + 1);
            }
        }
    }

    tree_impl(fat_fs, fat_fs.root_dir_iter(), show_hidden, 0);
}

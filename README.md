# FUSE implementation for the FAT file system

End-to-end implementation of a FAT driver, with the goal of being able to mount disk images with complete read-write support.

Uses (fuser)[https://docs.rs/fuser/latest/fuser/] as the underlying FUSE library.

## Specification

[https://academy.cba.mit.edu/classes/networking_communications/SD/FAT.pdf](https://academy.cba.mit.edu/classes/networking_communications/SD/FAT.pdf)

use super::syscall;

pub fn map_memory(
    virtual_address: Option<u32>,
    size: u32,
    physical_address: Option<u32>,
) -> Result<u32, ()> {
    let result = syscall(
        0x30,
        virtual_address.unwrap_or(0xffff_ffff),
        size,
        physical_address.unwrap_or(0xffff_ffff),
    );

    if result == 0xffff_ffff {
        Err(())
    } else {
        Ok(result)
    }
}

pub const MMAP_SHARED: u32 = 1;

#[repr(C)]
pub struct FileMapping {
    pub virtual_address: u32,
    pub size: u32,
    pub path_ptr: u32,
    pub path_len: u32,
    pub file_offset: u32,
    pub flags: u32,
}

pub fn map_file(
    virtual_address: Option<u32>,
    size: u32,
    path: &str,
    file_offset: u32,
    flags: u32,
) -> Result<u32, ()> {
    let path_bytes = path.as_bytes();
    let mapping = FileMapping {
        virtual_address: virtual_address.unwrap_or(0xffff_ffff),
        size,
        path_ptr: path_bytes.as_ptr() as u32,
        path_len: path_bytes.len() as u32,
        file_offset,
        flags,
    };

    let result = syscall(0x31, &mapping as *const FileMapping as u32, 0, 0);

    if result == 0xffff_ffff {
        Err(())
    } else {
        Ok(result)
    }
}

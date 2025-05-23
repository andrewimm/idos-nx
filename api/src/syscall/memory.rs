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

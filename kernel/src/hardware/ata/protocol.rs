use alloc::string::String;


#[repr(u8)]
pub enum AtaCommand {
    Identify = 0xec,
    ReadSectors = 0x20,
    WriteSectors = 0x30,
}

/// According to the ATA spec, each pair of bytes in an ATA string is "swapped"
/// This means that each word needs to be inverted, and the data for an ASCII
/// string cannot simply be copied directly from the raw buffer.
pub fn extract_ata_string(buffer: &[u16]) -> String {
    let mut converted = String::with_capacity(buffer.len());

    for pair in buffer.iter() {
        let low = *pair as u8;
        let high = (pair >> 8) as u8;
        converted.push(high as char);
        converted.push(low as char);
    }
    converted
}

//! IPV4, UDP, and TCP headers all have checksum fields that are computed in
//! the same manner. The relevant data is split into 16-bit segments which are
//! summed, with the carry being reapplied at the end. The entire sum is
//! inverted, revealing the checksum.
//! To compute a running checksum, create a new Checksum struct and add all the
//! u16 fields. When everything has been added, call `.compute()` to return the
//! 16-bit checksum number.

use super::{ip::IPV4Address, packet::PacketHeader};

/// Represents a running computation of a network header checksum
pub struct Checksum(u32);

impl Checksum {
    pub fn new() -> Self {
        Self(0)
    }

    /// Add another 16-bit field to the checksum total
    pub fn add_u16(&mut self, value: u16) {
        self.0 += value as u32;
    }

    /// Compute the final checksum value
    pub fn compute(&self) -> u16 {
        let mut running_sum = self.0;
        let carry = running_sum >> 16;
        running_sum &= 0xffff;
        running_sum += carry;
        if running_sum & 0xffff0000 != 0 {
            running_sum += 1;
        }

        (!running_sum) as u16
    }
}

/// UDP and TCP packets build their checksum with a pseudo-IPV4 header formed
/// from a subset of fields. This struct contains that header, and can be
/// cast as a u16 slice for adding to a Checksum.
#[repr(C, packed)]
pub struct IPChecksumHeader {
    pub source_ip: IPV4Address,
    pub dest_ip: IPV4Address,
    pub zeroes: u8,
    pub protocol: u8,
    pub udp_length: u16,
}

impl PacketHeader for IPChecksumHeader {}

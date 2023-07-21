/// AH=0x30 - Get DOS version
/// Input:
///     None
/// Output:
///     AL = Major version number
///     AH = Minor version number
pub fn get_version() -> (u8, u8) {
    // Some kind of setver capability may want to override this in the future
    (5, 0)
}

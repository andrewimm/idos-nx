use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::kprint!("{}\n", info);
    loop {}
}

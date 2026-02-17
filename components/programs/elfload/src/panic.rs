use core::panic::PanicInfo;

#[panic_handler]
pub fn panic(_info: &PanicInfo) -> ! {
    idos_api::syscall::exec::terminate(1);
}

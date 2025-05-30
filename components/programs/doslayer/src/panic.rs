use core::panic::PanicInfo;

#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    if let Some(message) = info.message().as_str() {
        let handle = idos_api::io::handle::Handle::new(1);
        let _ = idos_api::io::sync::write_sync(handle, message.as_bytes(), 0);
    }
    idos_api::syscall::exec::terminate(1);
}

#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}

use core::panic::PanicInfo;

#[panic_handler]
extern "C" fn panic(info: &PanicInfo) -> ! {
    let id = crate::task::switching::get_current_id();
    crate::kprint!("Kernel Panic in {:?} ", id);
    print_name(id);
    crate::kprint!("\n  {}\n", info);
    loop {}
}

#[inline(never)]
fn print_name(id: crate::task::id::TaskID) {
    match crate::task::switching::get_task(id) {
        Some(lock) => match lock.try_read() {
            Some(task) => {
                crate::kprint!("({})", task.filename.as_str());
                return;
            },
            _ => (),
        },
        _ => (),
    }
    crate::kprint!("(UNKNOWN)");
}


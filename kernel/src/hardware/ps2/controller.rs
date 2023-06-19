use crate::arch::port::Port;

pub fn read_ps2_data() -> u8 {
    Port::new(0x60).read_u8()
}

pub fn write_ps2_data(value: u8) {
    Port::new(0x60).write_u8(value);
}

pub fn get_ps2_status() -> u8 {
    Port::new(0x64).read_u8()
}

pub fn send_ps2_command(value: u8) {
    Port::new(0x64).write_u8(value);
}

pub fn data_read_ready() -> bool {
    let status = get_ps2_status();
    status & 1 != 0
}

pub fn data_write_ready() -> bool {
    let status = get_ps2_status();
    status & 2 == 0
}

pub fn get_configuration() -> u8 {
    send_ps2_command(0x20);
    while !data_read_ready() {}
    read_ps2_data()
}

pub fn set_configuration(value: u8) {
    send_ps2_command(0x60);
    while !data_write_ready() {}
    write_ps2_data(value);
}

pub fn initialize_controller() -> [bool; 2] {
    // disable both devices so that they don't cause any issues during init
    send_ps2_command(0xad);
    send_ps2_command(0xa7);
    // flush the output buffer, in case anything was there
    read_ps2_data();
    // get configuration value, so that it can be modified and eventually
    // restored to its original setting
    let mut config = get_configuration();
    // disable interrupts, translation
    config &= 0b10111100;
    set_configuration(config);

    // perform a self-test
    send_ps2_command(0xaa);
    while !data_read_ready() {}
    let response = read_ps2_data();
    if response != 0x55 {
        crate::kprint!("PS/2 Self-test failed!\n");
        return [false, false];
    }
    set_configuration(config);

    let mut device_ready = [true, true];

    // test the ports
    let tests: [u8; 2] = [0xab, 0xa9];
    for index in 0..tests.len() {
        send_ps2_command(0xab);
        while !data_read_ready() {}
        let result = read_ps2_data();
        if result == 0 {
            continue;
        }
        // else, some error was set
        crate::kprint!("PS/2 channel {} failed test\n", index);
        device_ready[index] = false;
    }

    // re-enable interrupts and devices
    config |= 3;
    set_configuration(config);
    send_ps2_command(0xae);
    send_ps2_command(0xa8);

    device_ready
}

pub fn reset_device() -> bool {
    while !data_write_ready() {}
    write_ps2_data(0xff);

    loop {
        while !data_read_ready() {}
        let response = read_ps2_data();
        if response == 0xfa {
            // read any trailing bytes (looking at you, mouse)
            while data_read_ready() {
                read_ps2_data();
            }
            return true;
        } else if response == 0xfc {
            return false;
        }
    }
}

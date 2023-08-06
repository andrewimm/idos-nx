#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use core::fmt::Write;

use idos_api::{
    syscall::{
        exec::{read_message_blocking, send_message, yield_coop},
        io::{read, write},
        net::{create_socket, bind_socket, socket_accept, socket_read},
    },
    io::handle::FileHandle,
};
use idos_sdk::driver::{AsyncDriver, IOError};

#[no_mangle]
pub extern fn main() {
    let stdin = FileHandle(0);
    let mut stdout = FileHandle(1);

    let config = match build_config() {
        Ok(c) => c,
        Err(err) => {
            stdout.write_str(err);
            stdout.write_char('\n');
            return;
        },
    };

    match config.host {
        Host::Local => run_local(stdin, stdout, config),
        Host::Remote(ip) => run_remote(stdin, stdout, config, ip),
    }

    let mut buffer: [u8; 256] = [0; 256];
    loop {
        let read_len = read(stdin, &mut buffer);

        write(stdout, &buffer[..read_len]);
    }
}

struct Config {
    host: Host,
    port: u16,
}

fn build_config() -> Result<Config, &'static str> {
    let mut args = idos_sdk::env::args();
    let mut config = Config {
        host: Host::Remote([0, 0, 0, 0]),
        port: 0,
    };

    loop {
        match args.next() {
            Some(option) => {
                match option {
                    "-l" => {
                        config.host = Host::Local;
                    },
                    _ => {
                        config.port = option.parse::<u16>().map_err(|_| "Invalid port number")?;
                    },
                }
            },
            None => break,
        }
    }

    if let Host::Remote([0, 0, 0, 0]) = config.host {
        return Err("Must specify a host, or use listener mode");
    }
    if config.port == 0 {
        return Err("Must specify a port number");
    }

    return Ok(config);
}

enum Host {
    Local,
    Remote([u8; 4]),
}

fn run_local(stdin: FileHandle, mut stdout: FileHandle, config: Config) {
    stdout.write_fmt(format_args!("Listening on local port {}\n", config.port));

    // create tcp socket
    let listener = create_socket(1);
    // bind to local port
    bind_socket(listener, [127, 0, 0, 1], config.port, [0, 0, 0, 0], 0);

    let connection = loop {
        match socket_accept(listener) {
            Some(conn) => break conn,
            None => (),
        }
    };
    stdout.write_str("Accepted connection\n");

    let mut buffer: [u8; 512] = [0; 512];
    loop {
        if let Some(len) = socket_read(connection, &mut buffer) {
            let s = core::str::from_utf8(&buffer[..len]).unwrap();
            stdout.write_str(s);
        }
        yield_coop();
    }

    /*
    let sock = net::socket::create_socket(net::socket::SocketProtocol::TCP);
    net::socket::bind_socket(sock, IPV4Address([127, 0, 0, 1]), SocketPort::new(84), IPV4Address([0, 0, 0, 0]), SocketPort::new(0)).unwrap();

    crate::kprintln!("Listening on 127.0.0.1:84");
    let connection = loop {
        match net::socket::socket_accept(sock) {
            Some(handle) => break handle,
            None => crate::task::actions::yield_coop(),
        }
    };
    crate::kprintln!("Accepted connection from remote endpoint");
        
    let mut buffer = alloc::vec::Vec::new();
    for _ in 0..1024 {
        buffer.push(0);
    }
    loop {
        if let Some(len) = net::socket::socket_read(connection, buffer.as_mut_slice()) {
            crate::kprintln!("GOT PAYLOAD");
            let s = core::str::from_utf8(&buffer[..len]).unwrap();
            crate::kprintln!("\"{}\"", s);
        }
        task::actions::yield_coop();
    }
    */
}

fn run_remote(stdin: FileHandle, mut stdout: FileHandle, config: Config, ip: [u8; 4]) {
    stdout.write_fmt(format_args!("Connected to remote host {}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], config.port));

    loop {
    }
}

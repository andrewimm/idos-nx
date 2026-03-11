#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::sync::{close_sync, open_sync, read_sync, stat_sync, write_sync};
use idos_api::io::Handle;
use idos_api::syscall::io::create_file_handle;
use idos_api::syscall::memory::map_memory;

const PAGE_SIZE: usize = 0x1000;
const SECTOR_SIZE: u32 = 512;

// 8 MB disk = 16384 sectors
const TOTAL_SECTORS: u16 = 16384;

// FAT12 geometry for an 8 MB disk
const SECTORS_PER_CLUSTER: u8 = 8;
const RESERVED_SECTORS: u16 = 1;
const FAT_COUNT: u8 = 2;
const SECTORS_PER_FAT: u16 = 8;
const ROOT_DIR_ENTRIES: u16 = 512;
const MEDIA_DESCRIPTOR: u8 = 0xF8;
const BYTES_PER_CLUSTER: u32 = SECTORS_PER_CLUSTER as u32 * SECTOR_SIZE;

// Derived layout (in sectors)
const FAT_START: u32 = RESERVED_SECTORS as u32;
const ROOT_DIR_START: u32 = FAT_START + FAT_COUNT as u32 * SECTORS_PER_FAT as u32;
const ROOT_DIR_SECTORS: u32 = (ROOT_DIR_ENTRIES as u32 * 32 + 511) / 512;
const DATA_START: u32 = ROOT_DIR_START + ROOT_DIR_SECTORS;
const TOTAL_CLUSTERS: u32 = (TOTAL_SECTORS as u32
    - RESERVED_SECTORS as u32
    - FAT_COUNT as u32 * SECTORS_PER_FAT as u32
    - ROOT_DIR_SECTORS)
    / SECTORS_PER_CLUSTER as u32;

// ---------------------------------------------------------------------------
// VGA text-mode console (0xB8000)
// ---------------------------------------------------------------------------

const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const VGA_ATTR: u8 = 0x07; // light grey on black
const VGA_HEADER_ATTR: u8 = 0x1F; // white on blue
const VGA_OK_ATTR: u8 = 0x0A; // green on black
const VGA_ERR_ATTR: u8 = 0x0C; // red on black

struct Vga {
    base: *mut u16,
    row: usize,
    col: usize,
    attr: u8,
}

impl Vga {
    fn init() -> Self {
        let addr = map_memory(None, PAGE_SIZE as u32, Some(0xB8000)).unwrap();
        let mut vga = Vga {
            base: addr as *mut u16,
            row: 0,
            col: 0,
            attr: VGA_ATTR,
        };
        vga.clear();
        vga
    }

    fn clear(&mut self) {
        let blank = (self.attr as u16) << 8 | b' ' as u16;
        for i in 0..VGA_WIDTH * VGA_HEIGHT {
            unsafe { *self.base.add(i) = blank; }
        }
        self.row = 0;
        self.col = 0;
    }

    fn scroll(&mut self) {
        // Move rows 1..HEIGHT up by one
        for row in 0..VGA_HEIGHT - 1 {
            for col in 0..VGA_WIDTH {
                let src = (row + 1) * VGA_WIDTH + col;
                let dst = row * VGA_WIDTH + col;
                unsafe { *self.base.add(dst) = *self.base.add(src); }
            }
        }
        // Clear last row
        let blank = (self.attr as u16) << 8 | b' ' as u16;
        for col in 0..VGA_WIDTH {
            unsafe { *self.base.add((VGA_HEIGHT - 1) * VGA_WIDTH + col) = blank; }
        }
        self.row = VGA_HEIGHT - 1;
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= VGA_HEIGHT {
            self.scroll();
        }
    }

    fn put_char(&mut self, ch: u8) {
        if ch == b'\n' {
            self.newline();
            return;
        }
        if self.col >= VGA_WIDTH {
            self.newline();
        }
        let offset = self.row * VGA_WIDTH + self.col;
        unsafe { *self.base.add(offset) = (self.attr as u16) << 8 | ch as u16; }
        self.col += 1;
    }

    fn print(&mut self, s: &[u8]) {
        for &ch in s {
            self.put_char(ch);
        }
    }

    fn print_str(&mut self, s: &str) {
        self.print(s.as_bytes());
    }

    fn print_num(&mut self, mut v: u32) {
        if v == 0 {
            self.put_char(b'0');
            return;
        }
        let mut buf = [0u8; 10];
        let mut pos = 0;
        while v > 0 {
            buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
            pos += 1;
        }
        for i in (0..pos).rev() {
            self.put_char(buf[i]);
        }
    }

    fn set_attr(&mut self, attr: u8) {
        self.attr = attr;
    }
}

// ---------------------------------------------------------------------------
// Keyboard input
// ---------------------------------------------------------------------------

struct Keyboard {
    handle: Handle,
}

/// Translate a KeyCode (from the kernel's PS/2 driver) to ASCII.
/// KeyCode values match the kernel's keycodes.rs enum.
fn keycode_to_ascii(kc: u8) -> u8 {
    match kc {
        0x08 => 0x08, // Backspace
        0x0d => b'\n', // Enter
        0x20 => b' ',  // Space
        0x30 => b'0', 0x31 => b'1', 0x32 => b'2', 0x33 => b'3', 0x34 => b'4',
        0x35 => b'5', 0x36 => b'6', 0x37 => b'7', 0x38 => b'8', 0x39 => b'9',
        0x41 => b'a', 0x42 => b'b', 0x43 => b'c', 0x44 => b'd', 0x45 => b'e',
        0x46 => b'f', 0x47 => b'g', 0x48 => b'h', 0x49 => b'i', 0x4a => b'j',
        0x4b => b'k', 0x4c => b'l', 0x4d => b'm', 0x4e => b'n', 0x4f => b'o',
        0x50 => b'p', 0x51 => b'q', 0x52 => b'r', 0x53 => b's', 0x54 => b't',
        0x55 => b'u', 0x56 => b'v', 0x57 => b'w', 0x58 => b'x', 0x59 => b'y',
        0x5a => b'z',
        _ => 0,
    }
}

impl Keyboard {
    fn open() -> Self {
        let handle = create_file_handle();
        open_sync(handle, "DEV:\\KEYBOARD", 0).unwrap();
        Keyboard { handle }
    }

    /// Read a key press from the keyboard. The PS/2 driver returns 2-byte
    /// events: [action, keycode] where action 1=press, 2=release.
    fn read_key(&self) -> u8 {
        let mut buf = [0u8; 2];
        loop {
            if let Ok(n) = read_sync(self.handle, &mut buf, 0) {
                if n >= 2 && buf[0] == 1 { // 1 = key press
                    let ascii = keycode_to_ascii(buf[1]);
                    if ascii != 0 {
                        return ascii;
                    }
                }
            }
            idos_api::syscall::exec::yield_coop();
        }
    }

    fn read_line(&self, vga: &mut Vga, buf: &mut [u8]) -> usize {
        let mut len = 0;
        loop {
            let ch = self.read_key();
            if ch == b'\n' {
                vga.newline();
                return len;
            }
            if ch == 0x08 {
                if len > 0 {
                    len -= 1;
                    if vga.col > 0 {
                        vga.col -= 1;
                        let offset = vga.row * VGA_WIDTH + vga.col;
                        unsafe { *vga.base.add(offset) = (vga.attr as u16) << 8 | b' ' as u16; }
                    }
                }
                continue;
            }
            if len < buf.len() && ch >= 0x20 {
                buf[len] = ch;
                len += 1;
                vga.put_char(ch);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn alloc_buf(size: usize) -> *mut u8 {
    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
    let total = pages * PAGE_SIZE;
    map_memory(None, total as u32, None).unwrap() as *mut u8
}

fn write_int_to_buf(buf: &mut [u8], mut v: u32) -> usize {
    if v == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut len = 0;
    while v > 0 {
        tmp[len] = b'0' + (v % 10) as u8;
        v /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

// ---------------------------------------------------------------------------
// In-memory FAT12 writer
// ---------------------------------------------------------------------------

struct Fat12Writer {
    disk: Handle,
    fat: *mut u8,
    fat_bytes: usize,
    root_dir: *mut u8,
    root_dir_bytes: usize,
    next_dir_entry: usize,
    next_free_cluster: u32,
}

impl Fat12Writer {
    fn new(disk: Handle) -> Self {
        let fat_bytes = SECTORS_PER_FAT as usize * 512;
        let fat = alloc_buf(fat_bytes);
        let fat_slice = unsafe { core::slice::from_raw_parts_mut(fat, fat_bytes) };
        for b in fat_slice.iter_mut() {
            *b = 0;
        }
        // FAT entries 0 and 1: media descriptor
        fat_slice[0] = MEDIA_DESCRIPTOR;
        fat_slice[1] = 0xFF;
        fat_slice[2] = 0xFF;

        let root_dir_bytes = ROOT_DIR_SECTORS as usize * 512;
        let root_dir = alloc_buf(root_dir_bytes);
        let dir_slice = unsafe { core::slice::from_raw_parts_mut(root_dir, root_dir_bytes) };
        for b in dir_slice.iter_mut() {
            *b = 0;
        }
        Fat12Writer {
            disk,
            fat,
            fat_bytes,
            root_dir,
            root_dir_bytes,
            next_dir_entry: 0,
            next_free_cluster: 2,
        }
    }

    fn fat_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.fat, self.fat_bytes) }
    }

    fn fat_slice_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.fat, self.fat_bytes) }
    }

    fn set_fat_entry(&mut self, cluster: u32, value: u16) {
        let fat = self.fat_slice_mut();
        let offset = (cluster / 2 * 3) as usize;
        if cluster & 1 == 0 {
            fat[offset] = value as u8;
            fat[offset + 1] = (fat[offset + 1] & 0xF0) | ((value >> 8) as u8 & 0x0F);
        } else {
            fat[offset + 1] = (fat[offset + 1] & 0x0F) | ((value as u8 & 0x0F) << 4);
            fat[offset + 2] = (value >> 4) as u8;
        }
    }

    fn allocate_chain(&mut self, num_clusters: u32) -> u32 {
        if num_clusters == 0 {
            return 0;
        }

        let mut first: u32 = 0;
        let mut prev: u32 = 0;
        let mut allocated: u32 = 0;
        let mut c = self.next_free_cluster;

        while allocated < num_clusters && c < TOTAL_CLUSTERS + 2 {
            let fat = self.fat_slice();
            let offset = (c / 2 * 3) as usize;
            let val = if c & 1 == 0 {
                (fat[offset] as u16) | (((fat[offset + 1] & 0x0F) as u16) << 8)
            } else {
                ((fat[offset + 1] >> 4) as u16) | ((fat[offset + 2] as u16) << 4)
            };

            if val == 0 {
                if first == 0 {
                    first = c;
                }
                if prev != 0 {
                    self.set_fat_entry(prev, c as u16);
                }
                prev = c;
                allocated += 1;
            }
            c += 1;
        }

        if allocated < num_clusters {
            return 0;
        }

        self.set_fat_entry(prev, 0xFFF);
        self.next_free_cluster = c;
        first
    }

    fn write_data(&self, first_cluster: u32, data: &[u8]) -> bool {
        let mut cluster = first_cluster;
        let mut offset = 0usize;

        while offset < data.len() {
            let disk_sector = DATA_START + (cluster - 2) * SECTORS_PER_CLUSTER as u32;
            let remaining = data.len() - offset;
            let chunk = remaining.min(BYTES_PER_CLUSTER as usize);

            if chunk < BYTES_PER_CLUSTER as usize {
                let buf_ptr = alloc_buf(BYTES_PER_CLUSTER as usize);
                let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, BYTES_PER_CLUSTER as usize) };
                for b in buf.iter_mut() {
                    *b = 0;
                }
                buf[..chunk].copy_from_slice(&data[offset..offset + chunk]);
                if write_sync(self.disk, buf, disk_sector * SECTOR_SIZE).is_err() {
                    return false;
                }
            } else if write_sync(self.disk, &data[offset..offset + chunk], disk_sector * SECTOR_SIZE).is_err() {
                return false;
            }

            offset += chunk;

            if offset < data.len() {
                let fat = self.fat_slice();
                let fo = (cluster / 2 * 3) as usize;
                cluster = if cluster & 1 == 0 {
                    (fat[fo] as u32) | (((fat[fo + 1] & 0x0F) as u32) << 8)
                } else {
                    ((fat[fo + 1] >> 4) as u32) | ((fat[fo + 2] as u32) << 4)
                };
            }
        }
        true
    }

    fn add_file(&mut self, name: &[u8; 11], data: &[u8]) -> bool {
        if self.next_dir_entry >= ROOT_DIR_ENTRIES as usize {
            return false;
        }

        let clusters_needed = if data.is_empty() {
            0
        } else {
            ((data.len() as u32) + BYTES_PER_CLUSTER - 1) / BYTES_PER_CLUSTER
        };

        let first_cluster = if clusters_needed > 0 {
            let c = self.allocate_chain(clusters_needed);
            if c == 0 {
                return false;
            }
            c
        } else {
            0
        };

        if clusters_needed > 0 && !self.write_data(first_cluster, data) {
            return false;
        }

        let entry_offset = self.next_dir_entry * 32;
        let dir = unsafe { core::slice::from_raw_parts_mut(self.root_dir, self.root_dir_bytes) };
        dir[entry_offset..entry_offset + 11].copy_from_slice(name);
        dir[entry_offset + 11] = 0x20; // archive attribute
        dir[entry_offset + 26] = first_cluster as u8;
        dir[entry_offset + 27] = (first_cluster >> 8) as u8;
        let size = data.len() as u32;
        dir[entry_offset + 28] = size as u8;
        dir[entry_offset + 29] = (size >> 8) as u8;
        dir[entry_offset + 30] = (size >> 16) as u8;
        dir[entry_offset + 31] = (size >> 24) as u8;

        self.next_dir_entry += 1;
        true
    }

    fn flush(&self) -> bool {
        let fat = self.fat_slice();
        if write_sync(self.disk, fat, FAT_START * SECTOR_SIZE).is_err() {
            return false;
        }
        if write_sync(self.disk, fat, (FAT_START + SECTORS_PER_FAT as u32) * SECTOR_SIZE).is_err() {
            return false;
        }
        let dir = unsafe { core::slice::from_raw_parts(self.root_dir, self.root_dir_bytes) };
        if write_sync(self.disk, dir, ROOT_DIR_START * SECTOR_SIZE).is_err() {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Boot sector + MBR stamping
// ---------------------------------------------------------------------------

fn build_boot_sector() -> [u8; 512] {
    let mut s = [0u8; 512];

    s[0] = 0xEB; s[1] = 0x3C; s[2] = 0x90;
    s[3..11].copy_from_slice(b"IDOS 1.0");

    let bps = SECTOR_SIZE as u16;
    s[11] = bps as u8; s[12] = (bps >> 8) as u8;
    s[13] = SECTORS_PER_CLUSTER;
    s[14] = RESERVED_SECTORS as u8; s[15] = (RESERVED_SECTORS >> 8) as u8;
    s[16] = FAT_COUNT;
    s[17] = ROOT_DIR_ENTRIES as u8; s[18] = (ROOT_DIR_ENTRIES >> 8) as u8;
    s[19] = TOTAL_SECTORS as u8; s[20] = (TOTAL_SECTORS >> 8) as u8;
    s[21] = MEDIA_DESCRIPTOR;
    s[22] = SECTORS_PER_FAT as u8; s[23] = (SECTORS_PER_FAT >> 8) as u8;
    s[24] = 63; s[25] = 0;
    s[26] = 255; s[27] = 0;

    s[36] = 0x80;
    s[38] = 0x29;
    s[39] = 0x49; s[40] = 0x44; s[41] = 0x4F; s[42] = 0x53;
    s[43..54].copy_from_slice(b"IDOS       ");
    s[54..62].copy_from_slice(b"FAT12   ");

    s[510] = 0x55; s[511] = 0xAA;
    s
}

fn write_boot_sector_with_mbr(disk: Handle) -> bool {
    let boot = build_boot_sector();
    if write_sync(disk, &boot, 0).is_err() {
        return false;
    }

    let mbr_handle = create_file_handle();
    if open_sync(mbr_handle, "A:\\MBR.BIN", 0).is_err() {
        return false;
    }
    let mut mbr_data = [0u8; 512];
    if read_sync(mbr_handle, &mut mbr_data, 0).is_err() {
        let _ = close_sync(mbr_handle);
        return false;
    }
    let _ = close_sync(mbr_handle);

    let mut sector = [0u8; 512];
    if read_sync(disk, &mut sector, 0).is_err() {
        return false;
    }
    sector[62..512].copy_from_slice(&mbr_data[62..512]);
    if write_sync(disk, &sector, 0).is_err() {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Read file from boot disk into buffer
// ---------------------------------------------------------------------------

fn read_file(path: &str) -> Option<(*mut u8, usize)> {
    let handle = create_file_handle();
    if open_sync(handle, path, 0).is_err() {
        return None;
    }

    let stat = match stat_sync(handle) {
        Ok(s) => s,
        Err(_) => {
            let _ = close_sync(handle);
            return None;
        }
    };
    let size = stat.byte_size as usize;
    if size == 0 {
        let _ = close_sync(handle);
        return None;
    }

    let buf = alloc_buf(size);
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, size) };

    let mut offset: u32 = 0;
    while (offset as usize) < size {
        let remaining = size - offset as usize;
        let chunk = remaining.min(4096);
        match read_sync(handle, &mut slice[offset as usize..offset as usize + chunk], offset) {
            Ok(n) => {
                if n == 0 { break; }
                offset += n;
            }
            Err(_) => break,
        }
    }
    let _ = close_sync(handle);

    if (offset as usize) < size {
        None
    } else {
        Some((buf, size))
    }
}

// ---------------------------------------------------------------------------
// Timezone picker
// ---------------------------------------------------------------------------

struct TimezoneEntry {
    name: &'static str,
    offset: i32,
}

const TIMEZONES: &[TimezoneEntry] = &[
    TimezoneEntry { name: "UTC-10  Hawaii", offset: -600 },
    TimezoneEntry { name: "UTC-9   Alaska", offset: -540 },
    TimezoneEntry { name: "UTC-8   Pacific (PST)", offset: -480 },
    TimezoneEntry { name: "UTC-7   Mountain (MST)", offset: -420 },
    TimezoneEntry { name: "UTC-6   Central (CST)", offset: -360 },
    TimezoneEntry { name: "UTC-5   Eastern (EST)", offset: -300 },
    TimezoneEntry { name: "UTC-4   Atlantic", offset: -240 },
    TimezoneEntry { name: "UTC-3   Argentina, Brazil", offset: -180 },
    TimezoneEntry { name: "UTC+0   UK, Portugal (GMT)", offset: 0 },
    TimezoneEntry { name: "UTC+1   Central Europe (CET)", offset: 60 },
    TimezoneEntry { name: "UTC+2   Eastern Europe (EET)", offset: 120 },
    TimezoneEntry { name: "UTC+3   Moscow", offset: 180 },
    TimezoneEntry { name: "UTC+5:30 India (IST)", offset: 330 },
    TimezoneEntry { name: "UTC+8   China, Singapore", offset: 480 },
    TimezoneEntry { name: "UTC+9   Japan, Korea (JST)", offset: 540 },
    TimezoneEntry { name: "UTC+10  Australia East (AEST)", offset: 600 },
    TimezoneEntry { name: "UTC+12  New Zealand (NZST)", offset: 720 },
];

fn pick_timezone(vga: &mut Vga, kbd: &Keyboard) -> i32 {
    vga.set_attr(VGA_ATTR);
    vga.print_str("\nSelect your timezone:\n\n");

    for (i, tz) in TIMEZONES.iter().enumerate() {
        if i + 1 < 10 {
            vga.put_char(b' ');
        }
        vga.print_num(i as u32 + 1);
        vga.print_str(") ");
        vga.print_str(tz.name);
        vga.put_char(b'\n');
    }

    loop {
        vga.print_str("\nEnter number (1-");
        vga.print_num(TIMEZONES.len() as u32);
        vga.print_str("): ");

        let mut buf = [0u8; 4];
        let len = kbd.read_line(vga, &mut buf);
        if len == 0 {
            continue;
        }

        let mut val: u32 = 0;
        let mut valid = true;
        for i in 0..len {
            if buf[i] >= b'0' && buf[i] <= b'9' {
                val = val * 10 + (buf[i] - b'0') as u32;
            } else {
                valid = false;
                break;
            }
        }

        if valid && val >= 1 && val <= TIMEZONES.len() as u32 {
            let tz = &TIMEZONES[(val - 1) as usize];
            vga.print_str("Selected: ");
            vga.print_str(tz.name);
            vga.put_char(b'\n');
            return tz.offset;
        }

        vga.print_str("Invalid selection.\n");
    }
}

// ---------------------------------------------------------------------------
// DRIVERS.CFG generation
// ---------------------------------------------------------------------------

fn build_drivers_cfg(tz_offset: i32) -> (*mut u8, usize) {
    let buf = alloc_buf(PAGE_SIZE);
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, PAGE_SIZE) };
    let mut pos = 0;

    let lines: &[&[u8]] = &[
        b"# IDOS Driver Configuration\n",
        b"# Generated by installer\n",
        b"\n",
        b"# Timezone\n",
    ];
    for line in lines {
        slice[pos..pos + line.len()].copy_from_slice(line);
        pos += line.len();
    }

    let prefix = b"timezone ";
    slice[pos..pos + prefix.len()].copy_from_slice(prefix);
    pos += prefix.len();
    if tz_offset < 0 {
        slice[pos] = b'-';
        pos += 1;
        pos += write_int_to_buf(&mut slice[pos..], (-tz_offset) as u32);
    } else {
        pos += write_int_to_buf(&mut slice[pos..], tz_offset as u32);
    }
    slice[pos] = b'\n';
    pos += 1;

    let more: &[&[u8]] = &[
        b"\n",
        b"# Hardware drivers\n",
        b"isa C:\\FLOPPY.ELF 6\n",
        b"isa C:\\SB16.ELF 5\n",
        b"pci 8086:100e C:\\E1000.ELF busmaster\n",
        b"\n",
        b"# Network stack\n",
        b"net\n",
        b"\n",
        b"# Additional filesystem mounts\n",
        b"mount A FAT FD1\n",
        b"\n",
        b"# Graphics driver\n",
        b"graphics C:\\GFX.ELF\n",
        b"\n",
        b"# Console manager\n",
        b"console\n",
    ];
    for line in more {
        slice[pos..pos + line.len()].copy_from_slice(line);
        pos += line.len();
    }

    (buf, pos)
}

// ---------------------------------------------------------------------------
// Convert "FILENAME.EXT" to FAT 8.3 name
// ---------------------------------------------------------------------------

fn make_fat_name(name: &str) -> [u8; 11] {
    let mut result = [b' '; 11];
    let bytes = name.as_bytes();
    let mut i = 0;
    let mut pos = 0;

    while i < bytes.len() && bytes[i] != b'.' && pos < 8 {
        result[pos] = bytes[i].to_ascii_uppercase();
        pos += 1;
        i += 1;
    }

    while i < bytes.len() && bytes[i] != b'.' {
        i += 1;
    }
    if i < bytes.len() {
        i += 1;
    }

    pos = 8;
    while i < bytes.len() && pos < 11 {
        result[pos] = bytes[i].to_ascii_uppercase();
        pos += 1;
        i += 1;
    }

    result
}

// ---------------------------------------------------------------------------
// Files to install
// ---------------------------------------------------------------------------

struct InstallFile {
    /// Path on the boot floppy (A:\ during install)
    src: &'static str,
    /// FAT 8.3 name for the hard disk
    name: &'static str,
}

// BOOT.BIN MUST be first — MBR requires it as the first root dir entry
const INSTALL_FILES: &[InstallFile] = &[
    InstallFile { src: "A:\\BOOT.BIN", name: "BOOT.BIN" },
    InstallFile { src: "A:\\KERNEL.BIN", name: "KERNEL.BIN" },
    InstallFile { src: "A:\\FATDRV.BIN", name: "FATDRV.BIN" },
    InstallFile { src: "A:\\COMMAND.ELF", name: "COMMAND.ELF" },
    InstallFile { src: "A:\\DOSLAYER.ELF", name: "DOSLAYER.ELF" },
    InstallFile { src: "A:\\ELFLOAD.ELF", name: "ELFLOAD.ELF" },
    InstallFile { src: "A:\\DISKCHK.ELF", name: "DISKCHK.ELF" },
    InstallFile { src: "A:\\GFX.ELF", name: "GFX.ELF" },
    InstallFile { src: "A:\\E1000.ELF", name: "E1000.ELF" },
    InstallFile { src: "A:\\FLOPPY.ELF", name: "FLOPPY.ELF" },
    InstallFile { src: "A:\\SB16.ELF", name: "SB16.ELF" },
    InstallFile { src: "A:\\NETCAT.ELF", name: "NETCAT.ELF" },
    InstallFile { src: "A:\\GOPHER.ELF", name: "GOPHER.ELF" },
    InstallFile { src: "A:\\TONEGEN.ELF", name: "TONEGEN.ELF" },
    InstallFile { src: "A:\\TERM14.PSF", name: "TERM14.PSF" },
];

// ---------------------------------------------------------------------------
// Main installer
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn main() {
    let mut vga = Vga::init();
    let kbd = Keyboard::open();

    // Header
    vga.set_attr(VGA_HEADER_ATTR);
    vga.print_str("                            IDOS Installation                                   ");
    vga.set_attr(VGA_ATTR);
    vga.print_str("\n\n");

    vga.print_str("This will install IDOS to the hard disk.\n");
    vga.print_str("ALL DATA ON THE HARD DISK WILL BE ERASED.\n\n");
    vga.print_str("Press ENTER to continue, or Q to quit: ");

    let ch = kbd.read_key();
    vga.put_char(b'\n');
    if ch == b'q' || ch == b'Q' {
        vga.print_str("\nInstallation cancelled.\n");
        return;
    }

    // --- Timezone ---
    let tz_offset = pick_timezone(&mut vga, &kbd);

    // --- Format ---
    vga.print_str("\nFormatting hard disk...\n");

    let disk = create_file_handle();
    if open_sync(disk, "DEV:\\ATA1", 0).is_err() {
        vga.set_attr(VGA_ERR_ATTR);
        vga.print_str("ERROR: Cannot open DEV:\\ATA1 - no hard disk found.\n");
        return;
    }

    vga.print_str("  Writing boot sector and MBR...");
    if !write_boot_sector_with_mbr(disk) {
        vga.set_attr(VGA_ERR_ATTR);
        vga.print_str(" FAILED\n");
        let _ = close_sync(disk);
        return;
    }
    vga.set_attr(VGA_OK_ATTR);
    vga.print_str(" OK\n");
    vga.set_attr(VGA_ATTR);

    // --- Create FAT12 writer and copy files ---
    let mut writer = Fat12Writer::new(disk);

    vga.print_str("\nCopying files...\n");

    let mut copied = 0u32;
    let mut failed = 0u32;

    for entry in INSTALL_FILES {
        vga.print_str("  ");
        vga.print_str(entry.name);
        // Pad to 16 chars
        let pad = if entry.name.len() < 16 { 16 - entry.name.len() } else { 1 };
        for _ in 0..pad {
            vga.put_char(b' ');
        }

        match read_file(entry.src) {
            Some((buf, size)) => {
                let data = unsafe { core::slice::from_raw_parts(buf, size) };
                let fat_name = make_fat_name(entry.name);
                if writer.add_file(&fat_name, data) {
                    vga.print_num(size as u32);
                    vga.print_str(" bytes ");
                    vga.set_attr(VGA_OK_ATTR);
                    vga.print_str("OK\n");
                    vga.set_attr(VGA_ATTR);
                    copied += 1;
                } else {
                    vga.set_attr(VGA_ERR_ATTR);
                    vga.print_str("FAILED\n");
                    vga.set_attr(VGA_ATTR);
                    failed += 1;
                }
            }
            None => {
                vga.set_attr(VGA_ERR_ATTR);
                vga.print_str("not found\n");
                vga.set_attr(VGA_ATTR);
                failed += 1;
            }
        }
    }

    // --- DRIVERS.CFG ---
    vga.print_str("  DRIVERS.CFG");
    for _ in 0..3 { vga.put_char(b' '); }
    let (cfg_buf, cfg_len) = build_drivers_cfg(tz_offset);
    let cfg_data = unsafe { core::slice::from_raw_parts(cfg_buf, cfg_len) };
    let cfg_name = make_fat_name("DRIVERS.CFG");
    if writer.add_file(&cfg_name, cfg_data) {
        vga.set_attr(VGA_OK_ATTR);
        vga.print_str("OK\n");
        vga.set_attr(VGA_ATTR);
        copied += 1;
    } else {
        vga.set_attr(VGA_ERR_ATTR);
        vga.print_str("FAILED\n");
        vga.set_attr(VGA_ATTR);
        failed += 1;
    }

    // --- Flush ---
    vga.print_str("\nWriting filesystem metadata...");
    if !writer.flush() {
        vga.set_attr(VGA_ERR_ATTR);
        vga.print_str(" FAILED\n");
        let _ = close_sync(disk);
        return;
    }
    vga.set_attr(VGA_OK_ATTR);
    vga.print_str(" OK\n");
    vga.set_attr(VGA_ATTR);

    let _ = close_sync(disk);

    // --- Summary ---
    vga.print_str("\n  Installed ");
    vga.print_num(copied);
    vga.print_str(" files");
    if failed > 0 {
        vga.print_str(", ");
        vga.set_attr(VGA_ERR_ATTR);
        vga.print_num(failed);
        vga.print_str(" failed");
        vga.set_attr(VGA_ATTR);
    }

    vga.put_char(b'\n');
    vga.set_attr(VGA_HEADER_ATTR);
    vga.print_str("\n  Installation complete!                                                        ");
    vga.set_attr(VGA_ATTR);
    vga.print_str("\n\nRemove the floppy disk and reboot to start IDOS.\n");
}

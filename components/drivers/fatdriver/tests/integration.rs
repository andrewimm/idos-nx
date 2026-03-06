use std::io::{Read, Seek, SeekFrom, Write};
use std::process::Command;

use fatdriver::disk::DiskIO;
use fatdriver::driver::FatDriver;

/// DiskIO implementation backed by a file on the host
struct FileDisk {
    file: std::fs::File,
}

impl FileDisk {
    fn new(path: &str) -> Self {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("failed to open disk image");
        Self { file }
    }
}

impl DiskIO for FileDisk {
    fn read(&mut self, buffer: &mut [u8], offset: u32) -> u32 {
        self.file.seek(SeekFrom::Start(offset as u64)).unwrap();
        self.file.read(buffer).unwrap() as u32
    }

    fn write(&mut self, buffer: &[u8], offset: u32) {
        self.file.seek(SeekFrom::Start(offset as u64)).unwrap();
        self.file.write_all(buffer).unwrap();
    }
}

fn get_timestamp() -> u32 {
    // Return a fixed timestamp for deterministic tests
    // 2024-01-15 12:00:00 as seconds since 1980-01-01
    // Approximate: 44 years * 365.25 * 86400 = ~1,388,534,400
    1_388_534_400
}

/// Create a fresh FAT12 disk image and return its path
fn create_test_disk(name: &str, size_kb: u32) -> String {
    let dir = std::env::temp_dir().join("fatdriver_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.img", name));
    let path_str = path.to_str().unwrap().to_string();

    // Remove if exists
    let _ = std::fs::remove_file(&path);

    // Create FAT12 disk image with mkfs.msdos
    let status = Command::new("mkfs.msdos")
        .args(["-C", &path_str, &size_kb.to_string()])
        .output()
        .expect("failed to run mkfs.msdos - is dosfstools installed?");

    assert!(status.status.success(), "mkfs.msdos failed: {}", String::from_utf8_lossy(&status.stderr));

    path_str
}

fn create_driver(disk_path: &str) -> FatDriver<FileDisk> {
    let disk_io = FileDisk::new(disk_path);
    FatDriver::new(disk_io, get_timestamp)
}

/// Use mtools to verify contents from the host side
fn mdir(disk_path: &str, path: &str) -> String {
    let output = Command::new("mdir")
        .args(["-i", disk_path, &format!("::{}", path)])
        .output()
        .expect("failed to run mdir");
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn mtype(disk_path: &str, path: &str) -> Vec<u8> {
    let output = Command::new("mtype")
        .args(["-i", disk_path, &format!("::{}", path)])
        .output()
        .expect("failed to run mtype");
    output.stdout
}

fn mcopy_to_image(disk_path: &str, content: &[u8], dest: &str) {
    let tmp = std::env::temp_dir().join("fatdriver_tests").join("tmp_file");
    std::fs::write(&tmp, content).unwrap();
    let status = Command::new("mcopy")
        .args(["-D", "o", "-i", disk_path, tmp.to_str().unwrap(), &format!("::{}", dest)])
        .output()
        .expect("failed to run mcopy");
    assert!(status.status.success(), "mcopy failed: {}", String::from_utf8_lossy(&status.stderr));
}

#[test]
fn test_open_root_directory() {
    let disk_path = create_test_disk("open_root", 1440);
    let mut driver = create_driver(&disk_path);

    // Open root directory (empty path)
    let result = driver.open("", 0);
    assert!(result.is_ok());

    let file_ref = result.unwrap();
    driver.close(file_ref).unwrap();
}

#[test]
fn test_create_and_read_file() {
    let disk_path = create_test_disk("create_read", 1440);
    let mut driver = create_driver(&disk_path);

    // Create a file
    let file_ref = driver.open("TEST.TXT", 0x1 /* OPEN_FLAG_CREATE */).unwrap();

    // Write some data
    let data = b"Hello, FAT filesystem!";
    let written = driver.write(file_ref, data, 0).unwrap();
    assert_eq!(written, data.len() as u32);

    // Close and reopen
    driver.close(file_ref).unwrap();

    let file_ref = driver.open("TEST.TXT", 0).unwrap();

    // Read back
    let mut buffer = [0u8; 64];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(read, data.len() as u32);
    assert_eq!(&buffer[..data.len()], data);

    driver.close(file_ref).unwrap();

    // Cross-check with mtools
    let content = mtype(&disk_path, "TEST.TXT");
    assert_eq!(content, data);
}

#[test]
fn test_read_preexisting_file() {
    let disk_path = create_test_disk("read_pre", 1440);

    // Write a file via mtools
    let expected = b"pre-existing content";
    mcopy_to_image(&disk_path, expected, "PRE.TXT");

    let mut driver = create_driver(&disk_path);
    let file_ref = driver.open("PRE.TXT", 0).unwrap();

    let mut buffer = [0u8; 64];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(read, expected.len() as u32);
    assert_eq!(&buffer[..expected.len()], expected);

    driver.close(file_ref).unwrap();
}

#[test]
fn test_file_not_found() {
    let disk_path = create_test_disk("not_found", 1440);
    let mut driver = create_driver(&disk_path);

    let result = driver.open("NOPE.TXT", 0);
    assert!(result.is_err());
}

#[test]
fn test_create_exclusive() {
    let disk_path = create_test_disk("exclusive", 1440);
    let mut driver = create_driver(&disk_path);

    // Create a file
    let file_ref = driver.open("EXCL.TXT", 0x3 /* CREATE | EXCLUSIVE */).unwrap();
    driver.close(file_ref).unwrap();

    // Trying to create again with EXCLUSIVE should fail
    let result = driver.open("EXCL.TXT", 0x3);
    assert!(result.is_err());
}

#[test]
fn test_directory_listing() {
    let disk_path = create_test_disk("dirlist", 1440);

    // Pre-populate with some files
    mcopy_to_image(&disk_path, b"aaa", "FILE1.TXT");
    mcopy_to_image(&disk_path, b"bbb", "FILE2.DAT");

    let mut driver = create_driver(&disk_path);
    let dir_ref = driver.open("", 0).unwrap();

    let mut buffer = [0u8; 256];
    let read = driver.read(dir_ref, &mut buffer, 0).unwrap();
    let listing = core::str::from_utf8(&buffer[..read as usize]).unwrap();

    // Entries are null-separated
    let entries: Vec<&str> = listing.split('\0').filter(|s| !s.is_empty()).collect();
    assert!(entries.contains(&"FILE1.TXT"));
    assert!(entries.contains(&"FILE2.DAT"));

    driver.close(dir_ref).unwrap();
}

#[test]
fn test_mkdir_and_subdir_operations() {
    let disk_path = create_test_disk("mkdir", 1440);
    let mut driver = create_driver(&disk_path);

    // Create a subdirectory
    driver.mkdir("SUBDIR").unwrap();

    // Create a file in the subdirectory
    let file_ref = driver.open("SUBDIR\\TEST.TXT", 0x1).unwrap();
    let data = b"subdir file";
    driver.write(file_ref, data, 0).unwrap();
    driver.close(file_ref).unwrap();

    // Read it back
    let file_ref = driver.open("SUBDIR\\TEST.TXT", 0).unwrap();
    let mut buffer = [0u8; 64];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(&buffer[..read as usize], data);
    driver.close(file_ref).unwrap();

    // Cross-check with mtools
    let content = mtype(&disk_path, "SUBDIR/TEST.TXT");
    assert_eq!(content, data);
}

#[test]
fn test_unlink() {
    let disk_path = create_test_disk("unlink", 1440);
    let mut driver = create_driver(&disk_path);

    // Create and then delete a file
    let file_ref = driver.open("DEL.TXT", 0x1).unwrap();
    driver.write(file_ref, b"delete me", 0).unwrap();
    driver.close(file_ref).unwrap();

    driver.unlink("DEL.TXT").unwrap();

    // File should no longer exist
    assert!(driver.open("DEL.TXT", 0).is_err());
}

#[test]
fn test_rmdir() {
    let disk_path = create_test_disk("rmdir", 1440);
    let mut driver = create_driver(&disk_path);

    driver.mkdir("EMPTYDIR").unwrap();
    driver.rmdir("EMPTYDIR").unwrap();

    // Directory should no longer exist
    assert!(driver.open("EMPTYDIR", 0).is_err());
}

#[test]
fn test_rmdir_nonempty_fails() {
    let disk_path = create_test_disk("rmdir_ne", 1440);
    let mut driver = create_driver(&disk_path);

    driver.mkdir("FULLDIR").unwrap();
    let file_ref = driver.open("FULLDIR\\FILE.TXT", 0x1).unwrap();
    driver.write(file_ref, b"content", 0).unwrap();
    driver.close(file_ref).unwrap();

    // Should fail because directory is not empty
    assert!(driver.rmdir("FULLDIR").is_err());
}

#[test]
fn test_rename() {
    let disk_path = create_test_disk("rename", 1440);
    let mut driver = create_driver(&disk_path);

    let file_ref = driver.open("OLD.TXT", 0x1).unwrap();
    driver.write(file_ref, b"rename test", 0).unwrap();
    driver.close(file_ref).unwrap();

    driver.rename("OLD.TXT", "NEW.TXT").unwrap();

    // Old name should not exist
    assert!(driver.open("OLD.TXT", 0).is_err());

    // New name should have the same content
    let file_ref = driver.open("NEW.TXT", 0).unwrap();
    let mut buffer = [0u8; 64];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(&buffer[..read as usize], b"rename test");
    driver.close(file_ref).unwrap();
}

#[test]
fn test_stat() {
    let disk_path = create_test_disk("stat", 1440);
    let mut driver = create_driver(&disk_path);

    // Create a file with known content
    let file_ref = driver.open("INFO.TXT", 0x1).unwrap();
    let data = b"stat test data here";
    driver.write(file_ref, data, 0).unwrap();
    driver.close(file_ref).unwrap();

    let file_ref = driver.open("INFO.TXT", 0).unwrap();
    let info = driver.stat(file_ref).unwrap();
    assert_eq!(info.byte_size, data.len() as u32);
    assert!(matches!(info.file_type, fatdriver::driver::FileTypeInfo::File));
    driver.close(file_ref).unwrap();

    // Stat a directory
    driver.mkdir("ADIR").unwrap();
    let dir_ref = driver.open("ADIR", 0).unwrap();
    let info = driver.stat(dir_ref).unwrap();
    assert!(matches!(info.file_type, fatdriver::driver::FileTypeInfo::Dir));
    driver.close(dir_ref).unwrap();
}

#[test]
fn test_write_at_offset() {
    let disk_path = create_test_disk("write_off", 1440);
    let mut driver = create_driver(&disk_path);

    let file_ref = driver.open("OFFSET.TXT", 0x1).unwrap();
    driver.write(file_ref, b"AAAA", 0).unwrap();
    driver.write(file_ref, b"BB", 2).unwrap();
    driver.close(file_ref).unwrap();

    let file_ref = driver.open("OFFSET.TXT", 0).unwrap();
    let mut buffer = [0u8; 10];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(&buffer[..read as usize], b"AABB");
    driver.close(file_ref).unwrap();
}

#[test]
fn test_file_growth() {
    let disk_path = create_test_disk("growth", 1440);
    let mut driver = create_driver(&disk_path);

    let file_ref = driver.open("GROW.TXT", 0x1).unwrap();

    // Write a pattern that spans multiple clusters (cluster = 512 bytes on FAT12 1440KB)
    let chunk = [0xABu8; 600];
    for i in 0..5 {
        driver.write(file_ref, &chunk, i * 600).unwrap();
    }
    driver.close(file_ref).unwrap();

    // Verify size
    let file_ref = driver.open("GROW.TXT", 0).unwrap();
    let info = driver.stat(file_ref).unwrap();
    assert_eq!(info.byte_size, 3000);

    // Read back all data
    let mut buffer = [0u8; 3000];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(read, 3000);
    assert!(buffer.iter().all(|&b| b == 0xAB));

    driver.close(file_ref).unwrap();
}

#[test]
fn test_rename_across_directories() {
    let disk_path = create_test_disk("rename_dirs", 1440);
    let mut driver = create_driver(&disk_path);

    driver.mkdir("SRCDIR").unwrap();
    driver.mkdir("DSTDIR").unwrap();

    let file_ref = driver.open("SRCDIR\\MOV.TXT", 0x1).unwrap();
    driver.write(file_ref, b"moving", 0).unwrap();
    driver.close(file_ref).unwrap();

    driver.rename("SRCDIR\\MOV.TXT", "DSTDIR\\MOV.TXT").unwrap();

    // Old location should not exist
    assert!(driver.open("SRCDIR\\MOV.TXT", 0).is_err());

    // New location should have the content
    let file_ref = driver.open("DSTDIR\\MOV.TXT", 0).unwrap();
    let mut buffer = [0u8; 32];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(&buffer[..read as usize], b"moving");
    driver.close(file_ref).unwrap();
}

#[test]
fn test_deep_directory_path() {
    let disk_path = create_test_disk("deep_path", 1440);
    let mut driver = create_driver(&disk_path);

    driver.mkdir("A").unwrap();
    driver.mkdir("A\\B").unwrap();
    driver.mkdir("A\\B\\C").unwrap();

    let file_ref = driver.open("A\\B\\C\\DEEP.TXT", 0x1).unwrap();
    driver.write(file_ref, b"deep", 0).unwrap();
    driver.close(file_ref).unwrap();

    let file_ref = driver.open("A\\B\\C\\DEEP.TXT", 0).unwrap();
    let mut buffer = [0u8; 16];
    let read = driver.read(file_ref, &mut buffer, 0).unwrap();
    assert_eq!(&buffer[..read as usize], b"deep");
    driver.close(file_ref).unwrap();
}

#[test]
fn test_mapping() {
    let disk_path = create_test_disk("mapping", 1440);
    let mut driver = create_driver(&disk_path);

    // Create a file with content
    let file_ref = driver.open("MAP.BIN", 0x1).unwrap();
    let data = vec![0x42u8; 256];
    driver.write(file_ref, &data, 0).unwrap();
    driver.close(file_ref).unwrap();

    // Create a mapping
    let token = driver.create_mapping("MAP.BIN").unwrap();

    // Page in
    let mut page = [0u8; 4096];
    let read = driver.page_in_mapping_to_buffer(token, 0, &mut page).unwrap();
    assert_eq!(read, 256);
    assert!(page[..256].iter().all(|&b| b == 0x42));
    assert!(page[256..].iter().all(|&b| b == 0));

    // Remove mapping
    driver.remove_mapping(token).unwrap();
}

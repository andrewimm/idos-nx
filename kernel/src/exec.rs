//! The exec module sets up a new task to run an executable program by mapping
//! a userspace loader into the task's address space. The loader is responsible
//! for parsing the actual executable and mapping its segments via syscalls.
//!
//! The kernel's role is minimal:
//! 1. Detect the executable format (ELF, COM, etc.)
//! 2. Map the appropriate loader binary into the target task
//! 3. Set up a "load info" page with the executable path and arguments
//! 4. Set up a stack and initial registers, then mark the task as runnable

use alloc::{string::String, vec::Vec};
use idos_api::io::driver::DriverMappingToken;
use spin::rwlock::RwLock;

use crate::{
    io::{
        filesystem::{driver::DriverID, driver_create_mapping},
        handle::Handle,
    },
    log::TaggedLogger,
    memory::{
        address::VirtualAddress, physical::allocate_frame_with_tracking,
        virt::scratch::UnmappedPage,
    },
    task::{
        actions::{
            handle::create_file_handle,
            io::{close_sync, open_sync, read_struct_sync},
            memory::map_memory_for_task,
        },
        id::TaskID,
        map::get_task,
        memory::MemoryBacking,
        paging::{ExternalPageDirectory, PermissionFlags},
    },
};

// === ELF Header Definitions ===

#[derive(Default)]
#[repr(C, packed)]
struct ElfHeader {
    magic: [u8; 4],
    bit_class: u8,
    endianness: u8,
    id_version: u8,
    os_abi: u8,
    abi_version: u8,
    _padding: [u8; 7],
    object_file_type: u16,
    machine: u16,
    elf_version: u32,
    entry_point: u32,
    program_header_offset: u32,
    section_header_offset: u32,
    flags: u32,
    header_size: u16,
    program_header_size: u16,
    program_header_count: u16,
    section_header_size: u16,
    section_header_count: u16,
    section_name_index: u16,
}

#[derive(Default)]
#[repr(C, packed)]
struct ProgramHeader {
    segment_type: u32,
    offset: u32,
    virtual_address: VirtualAddress,
    physical_address: u32,
    file_size: u32,
    memory_size: u32,
    flags: u32,
    alignment: u32,
}

const SEGMENT_TYPE_LOAD: u32 = 1;
const SEGMENT_FLAG_WRITE: u32 = 1 << 1;

const LOGGER: TaggedLogger = TaggedLogger::new("EXEC", 33);

const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];

/// Path to the ELF loader binary
/// TODO: we're accumulating a lot of these, we should move them to a central
/// configuration object
const ELF_LOADER_PATH: &str = "C:\\ELFLOAD.ELF";

/// Path to the DOS compatibility layer binary
const DOS_LOADER_PATH: &str = "C:\\DOSLAYER.ELF";

/// Stack is placed at the top of user address space
const STACK_TOP: u32 = 0xc000_0000;
const STACK_PAGES: u32 = 2;

/// Magic number written to the load info page header
const LOAD_INFO_MAGIC: u32 = 0x4C4F4144; // "LOAD"

/// Errors that can occur in the initial synchronous setup of execution
#[derive(Debug)]
pub enum ExecError {
    /// The requested executable file was not found or could not be opened
    FileNotFound,
    /// The executable format is not recognized or supported
    UnsupportedFormat,
    /// The loader binary could not be found or opened
    LoaderNotFound,
    /// The loader binary was found but its format was invalid (e.g. bad ELF headers)
    ParseError,
    /// Mapping of memory failed
    MappingFailed,
    /// The target task is not in the expected Uninitialized state
    InvalidTaskState,
    /// An internal error occurred
    InternalError,
}

/// Cached metadata about a loader binary, extracted from its ELF headers
/// When we support multiple executable formats, we'll need to extend this with
/// an enum for format-specific metadata
struct CachedLoader {
    /// Path used to identify this loader in the cache
    path: &'static str,
    driver_id: DriverID,
    mapping_token: DriverMappingToken,
    /// Entry point, relative to the base address of the first segment
    entry_point_offset: u32,
    /// Base virtual address from the ELF (the lowest segment vaddr, page-aligned)
    elf_base: u32,
    segments: Vec<CachedSegment>,
}

/// Metadata about a single loadable segment in the loader ELF. This is what we
/// cache for each segment to avoid repeatedly parsing the ELF headers.
struct CachedSegment {
    /// Virtual address offset relative to elf_base
    vaddr_offset: u32,
    /// Offset within the file
    file_offset: u32,
    /// Size in memory, rounded up to page boundary
    memory_size: u32,
    /// Whether this segment is writable (private mapping)
    writable: bool,
}

static LOADER_CACHE: RwLock<Vec<CachedLoader>> = RwLock::new(Vec::new());

/// Header written at the start of the load info page. This struct is shared
/// between the kernel and the userspace loader, so its layout must be stable.
#[repr(C)]
struct LoadInfoHeader {
    /// Magic number to identify the page and version of the format
    magic: u32,
    /// Offset within the page where the executable path string is stored
    exec_path_offset: u32,
    /// Length of the executable path string (not including null terminator)
    exec_path_len: u32,
    /// Number of arguments in the argv array
    argc: u32,
    /// Offset within the page where the argv data is stored (array of null-terminated strings)
    argv_offset: u32,
    /// Total length of all argument strings combined (for bounds checking)
    argv_total_len: u32,
}

const LOAD_INFO_DATA_START: usize = 0x100;

// === Public API ===

/// Set up a task to execute the program at `path`. This maps a userspace loader
/// into the task, writes a load info page, sets up a stack, and marks the task
/// as runnable. The loader will take over from there.
pub fn exec_program(task_id: TaskID, path: &str) -> Result<(), ExecError> {
    // 0. Verify the target task is in the expected state
    {
        let task_lock = get_task(task_id).ok_or(ExecError::InternalError)?;
        let task = task_lock.read();
        if !matches!(task.state, crate::task::state::RunState::Uninitialized) {
            LOGGER.log(format_args!(
                "exec {:?}: task is not Uninitialized, cannot exec",
                task_id
            ));
            return Err(ExecError::InvalidTaskState);
        }
    }

    // 1. Detect executable format by reading magic bytes
    let exec_handle = create_file_handle();
    let _ = open_sync(exec_handle, path).map_err(|_| ExecError::FileNotFound)?;

    let mut magic: [u8; 4] = [0; 4];
    let _ = crate::task::actions::io::read_sync(exec_handle, &mut magic, 0)
        .map_err(|_| ExecError::FileNotFound)?;
    let _ = close_sync(exec_handle);

    // 2. Pick the loader based on format
    let is_mz = magic[..2] == [b'M', b'Z'] || magic[..2] == [b'Z', b'M'];
    let loader_path = if magic == ELF_MAGIC {
        ELF_LOADER_PATH
    } else if is_mz || path.to_ascii_uppercase().ends_with(".COM") {
        DOS_LOADER_PATH
    } else {
        LOGGER.log(format_args!("Unsupported executable format: {:?}", magic));
        return Err(ExecError::UnsupportedFormat);
    };

    LOGGER.log(format_args!(
        "exec {:?}: format detected, using loader \"{}\"",
        task_id, loader_path
    ));

    // 3. Map the loader into the target task
    let (entry_point, _loader_base) = map_loader_for_task(task_id, loader_path)?;

    // 4. Set up the load info page
    let load_info_addr = setup_load_info_page(task_id, path)?;

    // 5. Set up stack
    setup_stack(task_id)?;

    // 6. Set registers and mark runnable
    {
        let task_lock = get_task(task_id).ok_or(ExecError::InternalError)?;
        let mut task = task_lock.write();

        task.set_filename(&String::from(path));

        // Push interrupt frame onto kernel stack
        task.stack_push_u32(0); // GS
        task.stack_push_u32(0); // FS
        task.stack_push_u32(0x20 | 3); // ES (user data segment)
        task.stack_push_u32(0x20 | 3); // DS
        task.stack_push_u32(0x20 | 3); // SS
        task.stack_push_u32(STACK_TOP); // ESP
        task.stack_push_u32(0); // EFLAGS
        task.stack_push_u32(0x18 | 3); // CS (user code segment)
        task.stack_push_u32(entry_point); // EIP — loader entry point
        task.stack_push_u32(0); // EDI
        task.stack_push_u32(0); // ESI
        task.stack_push_u32(0); // EBP
        task.stack_push_u32(load_info_addr.as_u32()); // EBX — load info page
        task.stack_push_u32(0); // EDX
        task.stack_push_u32(0); // ECX
        task.stack_push_u32(0); // EAX

        task.make_runnable();
        crate::task::scheduling::reenqueue_task(task.id);
    }

    LOGGER.log(format_args!(
        "exec {:?}: ready, EIP={:#010X} load_info={:?}",
        task_id, entry_point, load_info_addr
    ));

    Ok(())
}

// === Loader Mapping ===

/// Map the loader binary into the target task's address space. Returns the
/// absolute entry point address and the base address the loader was mapped at.
fn map_loader_for_task(
    task_id: TaskID,
    loader_path: &'static str,
) -> Result<(u32, VirtualAddress), ExecError> {
    // Check cache first
    let cache = LOADER_CACHE.read();
    if let Some(cached) = cache.iter().find(|c| c.path == loader_path) {
        let base = map_cached_loader(task_id, cached)?;
        let entry = base.as_u32() + cached.entry_point_offset;
        return Ok((entry, base));
    }
    drop(cache);

    // Cold path: parse the loader ELF and cache it
    let cached = parse_and_cache_loader(loader_path)?;

    let base = map_cached_loader(task_id, &cached)?;
    let entry = base.as_u32() + cached.entry_point_offset;

    LOADER_CACHE.write().push(cached);

    Ok((entry, base))
}

/// Use cached loader metadata to map segments into the target task. Returns
/// the base virtual address the loader was mapped at.
fn map_cached_loader(task_id: TaskID, cached: &CachedLoader) -> Result<VirtualAddress, ExecError> {
    // For a PIE loader, we map at the ELF's own base address for now.
    // In the future we could relocate, but the loader controls its own layout.
    let base = VirtualAddress::new(cached.elf_base);

    for segment in &cached.segments {
        let vaddr = base + segment.vaddr_offset;
        let shared = !segment.writable;

        let backing = MemoryBacking::FileBacked {
            driver_id: cached.driver_id,
            mapping_token: cached.mapping_token,
            offset_in_file: segment.file_offset,
            shared,
        };

        map_memory_for_task(task_id, Some(vaddr), segment.memory_size, backing)
            .map_err(|_| ExecError::MappingFailed)?;
    }

    LOGGER.log(format_args!(
        "Loader mapped at {:?} ({} segments)",
        base,
        cached.segments.len()
    ));

    Ok(base)
}

/// Parse the loader's ELF headers and create a driver mapping. This is the
/// cold path — only runs once per loader binary.
fn parse_and_cache_loader(loader_path: &'static str) -> Result<CachedLoader, ExecError> {
    LOGGER.log(format_args!("Parsing loader: {}", loader_path));

    // Open and read ELF headers
    let handle = create_file_handle();
    let _ = open_sync(handle, loader_path).map_err(|_| ExecError::LoaderNotFound)?;

    let mut elf_header = ElfHeader::default();
    let _ = read_struct_sync(handle, &mut elf_header, 0).map_err(|_| ExecError::ParseError)?;

    if elf_header.magic != ELF_MAGIC {
        let _ = close_sync(handle);
        return Err(ExecError::ParseError);
    }

    // Read program headers
    let mut program_headers: Vec<ProgramHeader> =
        Vec::with_capacity(elf_header.program_header_count as usize);
    let mut offset = elf_header.program_header_offset;
    for _ in 0..elf_header.program_header_count {
        let mut ph = ProgramHeader::default();
        let _ = read_struct_sync(handle, &mut ph, offset).map_err(|_| ExecError::ParseError)?;
        program_headers.push(ph);
        offset += elf_header.program_header_size as u32;
    }

    let _ = close_sync(handle);

    // Create a driver mapping for the loader file (kept alive for the cache)
    let (driver_id, relative_path) =
        crate::io::prepare_file_path(loader_path).map_err(|_| ExecError::LoaderNotFound)?;

    let result = match driver_create_mapping(driver_id, relative_path) {
        Some(immediate) => immediate,
        None => {
            // Async driver — need to suspend and wait, same as map_file_for_task
            let task_lock = crate::task::switching::get_current_task();
            task_lock.write().begin_file_mapping_request();
            crate::task::actions::yield_coop();
            let last_result = task_lock.write().last_map_result.take();
            match last_result {
                Some(r) => r,
                None => return Err(ExecError::LoaderNotFound),
            }
        }
    };
    let mapping_token = match result {
        Ok(token) => DriverMappingToken::new(token),
        Err(_) => return Err(ExecError::LoaderNotFound),
    };

    // Build cached segment list from PT_LOAD headers
    let mut segments: Vec<CachedSegment> = Vec::new();
    let mut elf_base: Option<u32> = None;

    for ph in &program_headers {
        if ph.segment_type != SEGMENT_TYPE_LOAD {
            continue;
        }

        let seg_vaddr = {
            let v = ph.virtual_address;
            v.as_u32()
        };
        let seg_vaddr_aligned = seg_vaddr & 0xfffff000;

        if elf_base.is_none() {
            elf_base = Some(seg_vaddr_aligned);
        }
        let base = elf_base.unwrap();

        // Page-align the file offset to match the vaddr alignment
        let file_offset_aligned = ph.offset & 0xfffff000;
        let memory_size_aligned =
            ((seg_vaddr + ph.memory_size + 0xfff) & 0xfffff000) - seg_vaddr_aligned;

        segments.push(CachedSegment {
            vaddr_offset: seg_vaddr_aligned - base,
            file_offset: file_offset_aligned,
            memory_size: memory_size_aligned,
            writable: ph.flags & SEGMENT_FLAG_WRITE != 0,
        });
    }

    let elf_base = elf_base.ok_or(ExecError::ParseError)?;
    let entry_point_offset = elf_header.entry_point - elf_base;

    LOGGER.log(format_args!(
        "Loader cached: base={:#X} entry_offset={:#X} segments={}",
        elf_base,
        entry_point_offset,
        segments.len()
    ));

    Ok(CachedLoader {
        path: loader_path,
        driver_id,
        mapping_token,
        entry_point_offset,
        elf_base,
        segments,
    })
}

// === Initial Memory Setup ===

/// Allocate a page in the target task's address space and fill it with
/// structured load information that the userspace loader will read.
fn setup_load_info_page(task_id: TaskID, exec_path: &str) -> Result<VirtualAddress, ExecError> {
    // Allocate a page of free memory in the target task
    let vaddr = map_memory_for_task(task_id, None, 0x1000, MemoryBacking::FreeMemory)
        .map_err(|_| ExecError::MappingFailed)?;

    // Eagerly page it so we can write to it
    let frame = allocate_frame_with_tracking().map_err(|_| ExecError::InternalError)?;
    let frame_paddr = frame.to_physical_address();
    let pagedir = ExternalPageDirectory::for_task(task_id);
    let flags = PermissionFlags::new(PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS);
    pagedir.map(vaddr, frame_paddr, flags);

    // Write load info via scratch page mapping
    {
        let scratch = UnmappedPage::map(frame_paddr);
        let page_ptr = scratch.virtual_address().as_ptr_mut::<u8>();
        let page_slice = unsafe { core::slice::from_raw_parts_mut(page_ptr, 0x1000) };

        // Zero the page first
        page_slice.fill(0);

        // Get task args
        let task_lock = get_task(task_id).ok_or(ExecError::InternalError)?;
        let task = task_lock.read();
        let args = task.args.arg_string();
        let argc = task.args.arg_count();
        let argv_len = args.len() as u32;

        // Write the executable path at LOAD_INFO_DATA_START
        let path_bytes = exec_path.as_bytes();
        let path_offset = LOAD_INFO_DATA_START;
        let path_end = path_offset + path_bytes.len();
        if path_end >= 0x1000 {
            return Err(ExecError::InternalError);
        }
        page_slice[path_offset..path_end].copy_from_slice(path_bytes);
        page_slice[path_end] = 0; // null terminate

        // Write argv data after the path
        let argv_offset = (path_end + 1 + 3) & !3; // align to 4 bytes
        let argv_end = argv_offset + args.len();
        if argv_end >= 0x1000 {
            return Err(ExecError::InternalError);
        }
        page_slice[argv_offset..argv_end].copy_from_slice(args);

        // Write the header
        let header = unsafe { &mut *(page_ptr as *mut LoadInfoHeader) };
        header.magic = LOAD_INFO_MAGIC;
        header.exec_path_offset = path_offset as u32;
        header.exec_path_len = path_bytes.len() as u32;
        header.argc = argc;
        header.argv_offset = argv_offset as u32;
        header.argv_total_len = argv_len;
    }

    LOGGER.log(format_args!("Load info page at {:?}", vaddr));

    Ok(vaddr)
}

/// Allocate stack pages for the target task.
fn setup_stack(task_id: TaskID) -> Result<(), ExecError> {
    let stack_size = STACK_PAGES * 0x1000;
    let stack_base = VirtualAddress::new(STACK_TOP - stack_size);

    map_memory_for_task(
        task_id,
        Some(stack_base),
        stack_size,
        MemoryBacking::FreeMemory,
    )
    .map_err(|_| ExecError::MappingFailed)?;

    Ok(())
}

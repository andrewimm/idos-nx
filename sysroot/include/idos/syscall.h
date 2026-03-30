#ifndef _IDOS_SYSCALL_H
#define _IDOS_SYSCALL_H

/*
 * Raw IDOS-NX system call interface.
 * Syscalls are invoked via INT 0x2b:
 *   EAX = syscall number
 *   EBX = arg0
 *   ECX = arg1
 *   EDX = arg2
 *   Returns: EAX = result
 */

static inline int __syscall(int num, int arg0, int arg1, int arg2) {
    int ret;
    __asm__ volatile(
        "int $0x2b"
        : "=a"(ret)
        : "a"(num), "b"(arg0), "c"(arg1), "d"(arg2)
        : "memory"
    );
    return ret;
}

static inline void __syscall2(int num, int arg0, int arg1, int arg2, int *out0, int *out1) {
    int ret0, ret1;
    __asm__ volatile(
        "int $0x2b"
        : "=a"(ret0), "=b"(ret1)
        : "a"(num), "b"(arg0), "c"(arg1), "d"(arg2)
        : "memory"
    );
    *out0 = ret0;
    *out1 = ret1;
}

/* Syscall numbers */
#define SYS_EXIT            0x00
#define SYS_YIELD           0x01
#define SYS_SLEEP           0x02
#define SYS_GET_TASK_ID     0x03
#define SYS_GET_PARENT_ID   0x04
#define SYS_ADD_ARGS        0x05
#define SYS_LOAD_EXEC       0x06
#define SYS_ENTER_8086      0x07

#define SYS_SUBMIT_IO       0x10
#define SYS_SEND_MESSAGE    0x11
#define SYS_DRIVER_COMPLETE 0x12
#define SYS_FUTEX_WAIT      0x13
#define SYS_FUTEX_WAKE      0x14
#define SYS_CREATE_WAKE_SET 0x15
#define SYS_BLOCK_WAKE_SET  0x16
#define SYS_DRAIN_WAKE_SET  0x17

#define SYS_CREATE_TASK     0x20
#define SYS_OPEN_MSG_QUEUE  0x21
#define SYS_OPEN_IRQ        0x22
#define SYS_CREATE_FILE_HANDLE 0x23
#define SYS_CREATE_PIPE     0x24
#define SYS_CREATE_UDP      0x25
#define SYS_CREATE_TCP      0x26

#define SYS_TRANSFER_HANDLE 0x2a
#define SYS_DUP_HANDLE      0x2b

#define SYS_MAP_MEMORY      0x30
#define SYS_MAP_FILE        0x31
#define SYS_UNMAP_MEMORY    0x32

#define SYS_GET_MONOTONIC   0x40
#define SYS_GET_SYSTEM_TIME 0x41

/* Async I/O operation codes */
#define IO_OP_OPEN   1
#define IO_OP_READ   2
#define IO_OP_WRITE  3
#define IO_OP_CLOSE  4
#define IO_OP_SHARE  5

#define FILE_OP_STAT  0x10
#define FILE_OP_IOCTL 0x11

/* Console IOCTL codes */
#define TSETGFX  0x6001
#define TSETTEXT 0x6002
#define TGETPAL  0x6003
#define TSETPAL  0x6004

/* Error flag — high bit set on return_value means error */
#define IO_ERROR_FLAG 0x80000000

/* No-handle / no-timeout sentinel */
#define IDOS_NONE 0xFFFFFFFF

/* Graphics mode request structure (matches kernel GraphicsMode) */
struct graphics_mode {
    unsigned short width;
    unsigned short height;
    unsigned int bpp_flags;
    unsigned int framebuffer;  /* filled by kernel on TSETGFX */
};

/* Async I/O operation structure */
struct async_op {
    unsigned int op_code;
    volatile unsigned int signal;
    volatile unsigned int return_value;
    unsigned int args[3];
};

/* Parameters for drain_wake_set (syscall 0x17) */
struct wake_batch_params {
    unsigned int buffer_ptr;
    unsigned int buffer_len;
    unsigned int timeout;
};

/* =========================================================================
 * Synchronous I/O helpers
 * ========================================================================= */

/* Perform synchronous I/O on a handle */
static inline int io_sync(int handle, int op_code, int arg0, int arg1, int arg2) {
    struct async_op op;
    op.op_code = op_code;
    op.signal = 0;
    op.return_value = 0;
    op.args[0] = arg0;
    op.args[1] = arg1;
    op.args[2] = arg2;

    if (__syscall(SYS_SUBMIT_IO, handle, (int)&op, -1) == (int)0x80000000) {
        return -1;
    }

    while (op.signal == 0) {
        __syscall(SYS_FUTEX_WAIT, (int)&op.signal, 0, -1);
    }

    return (int)op.return_value;
}

/* Submit an async I/O op with an optional wake set */
static inline int io_submit(int handle, struct async_op *op, int wake_set) {
    return __syscall(SYS_SUBMIT_IO, handle, (int)op, wake_set);
}

/* Initialize an async_op struct */
static inline void async_op_init(struct async_op *op, int op_code, int arg0, int arg1, int arg2) {
    op->op_code = op_code;
    op->signal = 0;
    op->return_value = 0;
    op->args[0] = arg0;
    op->args[1] = arg1;
    op->args[2] = arg2;
}

/* Open a file/device on a handle */
static inline int io_open(int handle, const char *path, int flags) {
    return io_sync(handle, IO_OP_OPEN, (int)path, __builtin_strlen(path), flags);
}

/* Read from a handle */
static inline int io_read(int handle, void *buffer, int length, int offset) {
    return io_sync(handle, IO_OP_READ, (int)buffer, length, offset);
}

/* Write to a handle */
static inline int io_write(int handle, const void *buffer, int length, int offset) {
    return io_sync(handle, IO_OP_WRITE, (int)buffer, length, offset);
}

/* Close a handle */
static inline int io_close(int handle) {
    return io_sync(handle, IO_OP_CLOSE, 0, 0, 0);
}

/* =========================================================================
 * Handle creation
 * ========================================================================= */

static inline int create_file_handle(void) {
    return __syscall(SYS_CREATE_FILE_HANDLE, 0, 0, 0);
}

static inline int create_message_queue(void) {
    return __syscall(SYS_OPEN_MSG_QUEUE, 0, 0, 0);
}

static inline int create_tcp_handle(void) {
    return __syscall(SYS_CREATE_TCP, 0, 0, 0);
}

static inline int create_udp_handle(void) {
    return __syscall(SYS_CREATE_UDP, 0, 0, 0);
}

static inline void create_pipe(int *read_handle, int *write_handle) {
    __syscall2(SYS_CREATE_PIPE, 0, 0, 0, read_handle, write_handle);
}

static inline int dup_handle(int handle) {
    return __syscall(SYS_DUP_HANDLE, handle, 0, 0);
}

static inline int transfer_handle(int handle, int dest_task) {
    return __syscall(SYS_TRANSFER_HANDLE, handle, dest_task, 0);
}

/* =========================================================================
 * Wake set (event multiplexing)
 * ========================================================================= */

/* Create a new wake set, returns its handle */
static inline int create_wake_set(void) {
    return __syscall(SYS_CREATE_WAKE_SET, 0, 0, 0);
}

/* Block until an IO handle in the set is ready. Returns the handle that
 * triggered the wake, or IDOS_NONE on timeout / spurious wake. */
static inline unsigned int wake_set_wait(int wake_set, unsigned int timeout_ms) {
    return (unsigned int)__syscall(SYS_BLOCK_WAKE_SET, wake_set, (int)timeout_ms, 0);
}

/* Block until at least one IO handle is ready, then drain all ready handles
 * into the buffer. Returns the number of handles written. */
static inline unsigned int wake_set_drain(int wake_set, unsigned int timeout_ms,
                                          unsigned int *buffer, unsigned int capacity) {
    struct wake_batch_params params;
    params.buffer_ptr = (unsigned int)buffer;
    params.buffer_len = capacity;
    params.timeout = timeout_ms;
    return (unsigned int)__syscall(SYS_DRAIN_WAKE_SET, wake_set, (int)&params, 0);
}

/* =========================================================================
 * Futex
 * ========================================================================= */

static inline void futex_wait(volatile unsigned int *addr, unsigned int expected, unsigned int timeout_ms) {
    __syscall(SYS_FUTEX_WAIT, (int)addr, (int)expected, (int)timeout_ms);
}

static inline void futex_wake(volatile unsigned int *addr, unsigned int count) {
    __syscall(SYS_FUTEX_WAKE, (int)addr, (int)count, 0);
}

/* =========================================================================
 * Task / process management
 * ========================================================================= */

static inline void task_exit(int code) {
    __syscall(SYS_EXIT, code, 0, 0);
}

static inline void task_yield(void) {
    __syscall(SYS_YIELD, 0, 0, 0);
}

static inline int get_task_id(void) {
    return __syscall(SYS_GET_TASK_ID, 0, 0, 0);
}

static inline int get_parent_id(void) {
    return __syscall(SYS_GET_PARENT_ID, 0, 0, 0);
}

/* =========================================================================
 * Memory management
 * ========================================================================= */

static inline int map_memory(unsigned int size, unsigned int phys_addr) {
    return __syscall(SYS_MAP_MEMORY, (int)IDOS_NONE, (int)size, (int)phys_addr);
}

static inline int map_memory_at(unsigned int vaddr, unsigned int size, unsigned int phys_addr) {
    return __syscall(SYS_MAP_MEMORY, (int)vaddr, (int)size, (int)phys_addr);
}

static inline int unmap_memory(unsigned int addr, unsigned int size) {
    return __syscall(SYS_UNMAP_MEMORY, (int)addr, (int)size, 0);
}

/* =========================================================================
 * Time
 * ========================================================================= */

static inline unsigned int get_monotonic_ms(void) {
    return (unsigned int)__syscall(SYS_GET_MONOTONIC, 0, 0, 0);
}

#endif

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

#define SYS_FUTEX_WAIT      0x13
#define SYS_FUTEX_WAKE      0x14
#define SYS_CREATE_WAKE_SET 0x15
#define SYS_BLOCK_WAKE_SET  0x16

#define SYS_CREATE_TASK     0x20
#define SYS_OPEN_MSG_QUEUE  0x21
#define SYS_OPEN_IRQ        0x22
#define SYS_CREATE_FILE_HANDLE 0x23
#define SYS_CREATE_PIPE     0x24

#define SYS_TRANSFER_HANDLE 0x2a
#define SYS_DUP_HANDLE      0x2b

#define SYS_MAP_MEMORY      0x30
#define SYS_MAP_FILE        0x31

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

/* Helper: perform synchronous I/O on a handle */
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

#endif

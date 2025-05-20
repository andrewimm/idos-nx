#include <stdatomic.h>

int syscall(int method, int arg0, int arg1, int arg2) {
  register int eax asm ("eax") = method;
  register int ebx asm ("ebx") = arg0;
  register int ecx asm ("ecx") = arg1;
  register int edx asm ("edx") = arg2;
  asm volatile (
      "int $0x2b"
      : "=r"(eax)
      : "r"(eax), "r"(ebx), "r"(ecx), "r"(edx)
  );
  return eax;
}

typedef struct async_op {
    int op_code;
    atomic_int signal;
    atomic_int return_value;
    int args[3];
} async_op_t;

int write_sync(int handle, char *buffer, int len, int offset) {
    async_op_t op;
    op.op_code = 3;
    op.signal = 0;
    op.return_value = 0;
    op.args[0] = (int) buffer;
    op.args[1] = len;
    op.args[2] = offset;

    syscall(0x10, handle, (int) &op, -1);

    while (atomic_load(&op.signal) == 0) {
        syscall(0x13, (int) &op.signal, 0, -1);
    }
}

int open_sync(int handle, char *path, int len) {
    async_op_t op;
    op.op_code = 1;
    op.signal = 0;
    op.return_value = 0;
    op.args[0] = (int) path;
    op.args[1] = len;
    op.args[2] = 0;

    syscall(0x10, handle, (int) &op, -1);

    while (atomic_load(&op.signal) == 0) {
        syscall(0x13, (int) &op.signal, 0, -1);
    }
}

void sleep(int ms) {
  syscall(2, ms, 0, 0);
}

void terminate(int code) {
  syscall(0, code, 0, 0);
}

int main(int argc, char* argv[]) {
  // temporary
  int stdin = syscall(0x23, 0, 0, 0);
  int stdout = syscall(0x23, 0, 0, 0);
  char *dev_con = "DEV:\\CON1";
  open_sync(stdout, dev_con, 9);

  char *label = "args: ";
  char *newline = "\n";
  write_sync(1, label, 6, 0);
  for (int i = 0; i < argc; i++) {
    char* arg_start = argv[i];
    int len = 0;
    while (*(arg_start + len) != 0) {
      len += 1;
    }
    write_sync(1, arg_start, len, 0);
    write_sync(1, newline, 1, 0);
  }
  sleep(5000);
  terminate(0);
}

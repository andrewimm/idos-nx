
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

void sleep(int ms) {
  syscall(5, ms, 0, 0);
}

void terminate(int code) {
  syscall(0, code, 0, 0);
}

void _start() {
  sleep(5000);
  terminate(0);
}

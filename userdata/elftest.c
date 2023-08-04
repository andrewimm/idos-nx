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

int main(int argc, char* argv[]) {
  char *label = "args: ";
  char *newline = "\n";
  syscall(0x13, 1, (int) label, 6);
  for (int i = 0; i < argc; i++) {
    char* arg_start = argv[i];
    int len = 0;
    while (*(arg_start + len) != 0) {
      len += 1;
    }
    syscall(0x13, 1, (int) arg_start, len);
    syscall(0x13, 1, (int) newline, 1);
  }
  sleep(5000);
  terminate(0);
}

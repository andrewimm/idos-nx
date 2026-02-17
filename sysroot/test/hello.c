#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char *argv[]) {
    printf("Hello from IDOS-NX libc!\n");
    printf("argc = %d\n", argc);
    for (int i = 0; i < argc; i++) {
        printf("argv[%d] = %s\n", i, argv[i]);
    }

    /* Test malloc */
    char *buf = malloc(128);
    if (buf) {
        strcpy(buf, "malloc works!");
        printf("%s\n", buf);
        free(buf);
    }

    /* Test snprintf */
    char fmt_buf[64];
    snprintf(fmt_buf, sizeof(fmt_buf), "formatted: %d %x %s", 42, 0xdead, "test");
    printf("%s\n", fmt_buf);

    return 0;
}

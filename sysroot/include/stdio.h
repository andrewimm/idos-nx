#ifndef _STDIO_H
#define _STDIO_H

#include <stddef.h>
#include <stdarg.h>

#define EOF (-1)
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define BUFSIZ 1024
#define FILENAME_MAX 256
#define L_tmpnam 20
#define TMP_MAX 1000

#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2

typedef struct _FILE FILE;

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

FILE *fopen(const char *path, const char *mode);
FILE *freopen(const char *path, const char *mode, FILE *stream);
int fclose(FILE *stream);
size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);
int fseek(FILE *stream, int offset, int whence);
int ftell(FILE *stream);
int feof(FILE *stream);
int ferror(FILE *stream);
void clearerr(FILE *stream);
int fflush(FILE *stream);
void rewind(FILE *stream);

int fgetc(FILE *stream);
int getc(FILE *stream);
int ungetc(int c, FILE *stream);
char *fgets(char *s, int n, FILE *stream);

int fputc(int c, FILE *stream);
int putc(int c, FILE *stream);
int putchar(int c);
int fputs(const char *s, FILE *stream);
int puts(const char *s);

int printf(const char *format, ...) __attribute__((format(printf, 1, 2)));
int fprintf(FILE *stream, const char *format, ...) __attribute__((format(printf, 2, 3)));
int sprintf(char *str, const char *format, ...) __attribute__((format(printf, 2, 3)));
int snprintf(char *str, size_t size, const char *format, ...) __attribute__((format(printf, 3, 4)));

int vprintf(const char *format, va_list ap);
int vfprintf(FILE *stream, const char *format, va_list ap);
int vsprintf(char *str, const char *format, va_list ap);
int vsnprintf(char *str, size_t size, const char *format, va_list ap);

int sscanf(const char *str, const char *format, ...);

int remove(const char *pathname);
int rename(const char *oldpath, const char *newpath);
FILE *tmpfile(void);
char *tmpnam(char *s);

int fileno(FILE *stream);
void setbuf(FILE *stream, char *buf);
int setvbuf(FILE *stream, char *buf, int mode, size_t size);

void perror(const char *s);

#endif

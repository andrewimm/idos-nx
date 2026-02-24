#ifndef _ERRNO_H
#define _ERRNO_H

extern int errno;
int *__errno_location(void);

#define ENOENT  2
#define EIO     5
#define EBADF   9
#define ENOMEM  12
#define EACCES  13
#define EEXIST  17
#define EISDIR  21
#define ENOTDIR 20
#define EINVAL  22
#define EMFILE  24
#define ENOSPC  28
#define ERANGE  34
#define ENOSYS  38

#endif

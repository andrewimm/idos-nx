#ifndef _TERMIOS_H
#define _TERMIOS_H

typedef unsigned int tcflag_t;
typedef unsigned char cc_t;
typedef unsigned int speed_t;

#define NCCS 20

struct termios {
    tcflag_t c_iflag;   /* input modes */
    tcflag_t c_oflag;   /* output modes */
    tcflag_t c_cflag;   /* control modes */
    tcflag_t c_lflag;   /* local modes */
    cc_t     c_cc[NCCS]; /* control characters */
};

/* c_iflag bits */
#define IGNBRK  0x0001
#define BRKINT  0x0002
#define IGNPAR  0x0004
#define INPCK   0x0010
#define ISTRIP  0x0020
#define INLCR   0x0040
#define IGNCR   0x0080
#define ICRNL   0x0100
#define IXON    0x0400
#define IXOFF   0x1000

/* c_oflag bits */
#define OPOST   0x0001
#define ONLCR   0x0004

/* c_cflag bits */
#define CS8     0x0030
#define CREAD   0x0080
#define CLOCAL  0x8000

/* c_lflag bits */
#define ISIG    0x0001
#define ICANON  0x0002
#define ECHO    0x0008
#define ECHOE   0x0010
#define ECHOK   0x0020
#define ECHONL  0x0040
#define IEXTEN  0x8000

/* c_cc indices */
#define VEOF    0
#define VEOL    1
#define VERASE  2
#define VKILL   3
#define VINTR   4
#define VQUIT   5
#define VSUSP   6
#define VSTART  7
#define VSTOP   8
#define VMIN    9
#define VTIME   10

/* tcsetattr actions */
#define TCSANOW   0
#define TCSADRAIN 1
#define TCSAFLUSH 2

int tcgetattr(int fd, struct termios *termios_p);
int tcsetattr(int fd, int optional_actions, const struct termios *termios_p);
speed_t cfgetispeed(const struct termios *termios_p);
speed_t cfgetospeed(const struct termios *termios_p);
int cfsetispeed(struct termios *termios_p, speed_t speed);
int cfsetospeed(struct termios *termios_p, speed_t speed);

#endif

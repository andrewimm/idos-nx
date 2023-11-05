# Async IO

System IO is *async first*. It's easier to wrap an async system in a blocking,
sync wrapper than it is to hack some version of async behavior on top of a sync
syscall.

It turns out that most programs end up waiting for async IO. This might be
waiting for user input, data availability, or a hardware interrupt. Programs
often also wait for a set of conditions -- ie, a terminal program that blocks
until the user types on the keyboard, or new output data is available. The
Async IO APIs provided by the IDOS kernel make these situations easy to handle.

Async IO interfaces are available for a wide range of data sources that are not
instantaneous, including:
 - Waiting for a child task to complete
 - Listening for an incoming IPC Message
 - Reading or writing to a Filesystem or Device Driver
 - Sending and receiving packets through a network socket
 - Waiting for a hardware interrupt
 - Waiting for a soft interrupt, similar to a UNIX signal

Being async by default also means that individual tasks can perform IO without
blocking. This makes it easier to implement multithreading in userspace, since
IDOS deliberately does not have a concept of kernel-level threads.

The syscall API size is deliberately small. Once an Async IO instance has been
created, all interactions happen by passing an instruction packet to the
kernel. This packet is known as an Operation, or "Op." Each open IO instance
processes a queue of Ops in a first-come-first-serve manner. When the operation
is completed, the originating task will be informed by a shared semaphore used
for signaling.

The default way to wait for an Op to complete is to poll the completion
semaphore, yielding after each read. However, this will dedicate a lot of CPU
time to the waiting task. It would be more efficient to block until the Op is
ready, similar to a traditional sync/blocking IO API. IDOS provides a concept
of Notify Queues, similar to BSD kqueues. Multiple handles can be added to a
Notify Queue, and a Task may have any number of queues. The Task can block on a
queue, and it will not run again until any of the handles in the queue
completes a pending Op.


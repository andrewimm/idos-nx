set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR i386)

# Use system GCC in 32-bit mode
set(CMAKE_C_COMPILER gcc)
set(CMAKE_C_STANDARD 99)

# Sysroot path (set to absolute path of sysroot/)
get_filename_component(IDOS_SYSROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
set(CMAKE_SYSROOT "${IDOS_SYSROOT}")

# Compiler flags
set(CMAKE_C_FLAGS_INIT
    "-m32 -march=i386 -ffreestanding -nostdlib -nostartfiles -fno-stack-protector -fno-exceptions -mno-sse -mno-mmx"
)

# Use our sysroot for headers
set(CMAKE_C_FLAGS_INIT "${CMAKE_C_FLAGS_INIT} -isystem ${IDOS_SYSROOT}/include")

# Linker flags
set(CMAKE_EXE_LINKER_FLAGS_INIT
    "-m32 -static -nostdlib -nostartfiles ${IDOS_SYSROOT}/lib/crt0.o -L${IDOS_SYSROOT}/lib -lc -lgcc"
)

# Search only in sysroot for libraries and includes
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)

# Don't try to compile test programs (they'd fail without our crt0/libc)
set(CMAKE_C_COMPILER_WORKS TRUE)
set(CMAKE_CXX_COMPILER_WORKS TRUE)

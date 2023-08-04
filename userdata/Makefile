.PHONY: all, clean

all: disk/dosio.com disk/elftest.elf disk/test.bin

clean:
	@rm -r build
	@rm -r disk

disk/test.bin: testbin.s
	@mkdir -p build
	@mkdir -p disk
	@as --32 -march=i386 -o build/testbin.o testbin.s
	@strip -o build/testbin-strip.o --remove-section=".note.*" --keep-symbol=start build/testbin.o
	@objcopy -I elf32-i386 -O binary build/testbin-strip.o disk/test.bin

disk/elftest.elf: elftest.c
	@mkdir -p build
	@mkdir -p disk
	@gcc -shared -nostdlib -nodefaultlibs -fno-exceptions -fno-stack-protector -nostartfiles -fPIE -z noseparate-code -march=i386 -m32 -Wl,-static -Wl,-Bsymbolic -o disk/elftest.elf crt0.s elftest.c

disk/dosio.com: dosio.s
	@mkdir -p build
	@mkdir -p disk
	@as --32 -march=i386 -o build/dosio.o dosio.s
	@strip -o build/dosio-strip.o --remove-section=".note.*" -x build/dosio.o
	@ld -o disk/dosio.com --oformat binary -e start -m elf_i386 -Ttext 0x100 build/dosio-strip.o


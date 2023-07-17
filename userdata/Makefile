.PHONY: all, clean

all: build/dosio.com build/test.bin

clean:
	@rm -r build
	@rm -r disk

build/test.bin: testbin.s
	@mkdir -p build
	@mkdir -p disk
	@as --32 -march=i386 -o build/testbin.o testbin.s
	@strip -o build/testbin-strip.o --remove-section=".note.*" --keep-symbol=start build/testbin.o
	@objcopy -I elf32-i386 -O binary build/testbin-strip.o disk/test.bin

build/dosio.com: dosio.s
	@mkdir -p build
	@mkdir -p disk
	@as --32 -march=i386 -o build/dosio.o dosio.s
	@strip -o build/dosio-strip.o --remove-section=".note.*" --keep-symbol=start build/dosio.o
	@ld -o disk/dosio.com --oformat binary -e start -m elf_i386 -Ttext 0x100 build/dosio-strip.o


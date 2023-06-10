.PHONY: all, clean

all: build/test.bin

clean:
	@rm -r build
	@rm -r disk

build/test.bin: testbin.s
	@mkdir -p build
	@mkdir -p disk
	@as --32 -march=i386 -o build/testbin.o testbin.s
	@strip -o build/testbin-strip.o --remove-section=".note.*" --keep-symbol=start build/testbin.o
	@objcopy -I elf32-i386 -O binary build/testbin-strip.o disk/test.bin


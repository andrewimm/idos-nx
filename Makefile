diskimage := build/bootdisk.img
userdata := build/datadisk.img
bootsector := build/mbr.bin
bootbin := build/boot.bin
kernel := build/kernel.bin

command := target/i386-idos/release/command
doslayer := target/i386-idos/release/doslayer
elfload := target/i386-idos/release/elfload
gfx := target/i386-idos/release/gfx

kernel_build_flags := --release -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target i386-kernel.json


.PHONY: all, clean, run

all: bootdisk

clean:
	@rm -r build

run: bootdisk
	@qemu-system-i386 -m 8M -drive format=raw,file=$(diskimage) -serial stdio -fda $(userdata) -device floppy,unit=1,drive= -device isa-debug-exit,iobase=0xf4,iosize=4 -display sdl; \
	EXIT_CODE=$$?; \
	exit $$(($$EXIT_CODE >> 1))

$(diskimage):
	@mkdir -p $(shell dirname $@)
	@mkfs.msdos -C $(diskimage) 1440

$(userdata):
	@mkdir -p $(shell dirname $@)
	@mkdir -p userdata/disk
	@mkfs.msdos -C $(userdata) 1440
	@cd userdata && make
	@mcopy -D o -i $(userdata) userdata/disk/*.* ::
	@mcopy -D o -i $(userdata) userdata/static/*.* ::

bootdisk: $(command) $(doslayer) $(elfload) $(gfx) $(diskimage) $(userdata) $(bootsector) $(bootbin) $(kernel)
	@dd if=$(bootsector) of=$(diskimage) bs=450 count=1 seek=62 skip=62 iflag=skip_bytes oflag=seek_bytes conv=notrunc
	@mcopy -D o -i $(diskimage) $(bootbin) ::BOOT.BIN
	@mcopy -D o -i $(diskimage) $(kernel) ::KERNEL.BIN
	@mcopy -D o -i $(diskimage) $(command) ::COMMAND.ELF
	@mcopy -D o -i $(diskimage) $(doslayer) ::DOSLAYER.ELF
	@mcopy -D o -i $(diskimage) $(elfload) ::ELFLOAD.ELF
	@mcopy -D o -i $(diskimage) $(gfx) ::GFX.ELF
	@mcopy -D o -i $(diskimage) resources/ter-i14n.psf ::TERM14.PSF

$(bootsector):
	@mkdir -p $(shell dirname $@)
	@cd bootloader/mbr && \
	cargo build --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem --target i386-mbr.json
	@objcopy -I elf32-i386 -O binary target/i386-mbr/release/idos-mbr $(bootsector)

$(bootbin):
	@mkdir -p $(shell dirname $@)
	@cd bootloader/bootbin && \
	cargo build --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem --target i386-bootbin.json
	@objcopy -I elf32-i386 -O binary target/i386-bootbin/release/idos-bootbin $(bootbin)

$(kernel):
	@mkdir -p $(shell dirname $@)
	@cd kernel && \
	cargo build $(kernel_build_flags)
	@cp target/i386-kernel/release/idos_kernel $(kernel)

testkernel:
	@mkdir -p build
	@cd kernel && \
	cargo test --no-run $(kernel_build_flags) &>../testkernel.log
	TEST_EXEC=$(shell grep -Po "Executable unittests src/main.rs \(\K[^\)]+" testkernel.log); \
	cp $$TEST_EXEC $(kernel)

test: testkernel run

$(command):
	@cd components/programs/command && \
	cargo build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target ../../i386-idos.json --release

$(doslayer):
	@cd components/programs/doslayer && \
	cargo build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target ../../i386-idos.json --release

$(elfload):
	@cd components/programs/elfload && \
	cargo build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target ../../i386-idos.json --release

$(gfx):
	@cd components/programs/gfx && \
	cargo build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target ../../i386-idos.json --release

diskimage := build/bootdisk.img
bootsector := build/mbr.bin
bootbin := build/boot.bin
kernel := build/kernel.bin

kernel_build_flags := --release -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target i386-kernel.json


.PHONY: all, clean, run

all: bootdisk

clean:
	@rm -r build

run: bootdisk
	@qemu-system-i386 -m 8M -drive format=raw,file=$(diskimage) -serial stdio

$(diskimage):
	@mkdir -p $(shell dirname $@)
	@mkfs.msdos -C $(diskimage) 1440

bootdisk: $(diskimage) $(bootsector) $(bootbin) $(kernel)
	@dd if=$(bootsector) of=$(diskimage) bs=450 count=1 seek=62 skip=62 iflag=skip_bytes oflag=seek_bytes conv=notrunc
	@mcopy -D o -i $(diskimage) $(bootbin) ::BOOT.BIN
	@mcopy -D o -i $(diskimage) $(kernel) ::KERNEL.BIN

$(bootsector):
	@mkdir -p $(shell dirname $@)
	@cd bootloader/mbr && \
	cargo build --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem --target i386-mbr.json
	@objcopy -I elf32-i386 -O binary bootloader/mbr/target/i386-mbr/release/idos-mbr $(bootsector)

$(bootbin):
	@mkdir -p $(shell dirname $@)
	@cd bootloader/bootbin && \
	cargo build --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem --target i386-bootbin.json
	@objcopy -I elf32-i386 -O binary bootloader/bootbin/target/i386-bootbin/release/idos-bootbin $(bootbin)

$(kernel):
	@mkdir -p $(shell dirname $@)
	@cd kernel && \
	cargo build $(kernel_build_flags)
	@cp kernel/target/i386-kernel/release/idos_kernel $(kernel)

testkernel:
	@mkdir -p build
	@cd kernel && \
	cargo test --no-run $(kernel_build_flags) &>../testkernel.log
	TEST_EXEC=$(shell grep -Po "Executable unittests src/main.rs \(\K[^\)]+" testkernel.log); \
	cp kernel/$$TEST_EXEC $(kernel)

test: testkernel run

diskimage := build/bootdisk.img
bootsector := build/mbr.bin
bootbin := build/boot.bin

.PHONY: all, clean

all: bootdisk

clean:
	@rm -r build

$(diskimage):
	@mkdir -p $(shell dirname $@)
	@mkfs.msdos -C $(diskimage) 1440

bootdisk: $(diskimage) $(bootsector) $(bootbin)
	@dd if=$(bootsector) of=$(diskimage) bs=450 count=1 seek=62 skip=62 iflag=skip_bytes oflag=seek_bytes conv=notrunc
	@mcopy -D o -i $(diskimage) $(bootbin) ::BOOT.BIN

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

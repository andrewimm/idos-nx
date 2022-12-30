diskimage := build/bootdisk.img
bootsector := build/boot.bin

.PHONY: all, clean

all: bootdisk

clean:
	@rm -r build

$(diskimage):
	@mkdir -p $(shell dirname $@)
	@mkfs.msdos -C $(diskimage) 1440

bootdisk: $(diskimage) $(bootsector)
	@dd if=$(bootsector) of=$(diskimage) bs=450 count=1 seek=62 skip=62 iflag=skip_bytes oflag=seek_bytes conv=notrunc

$(bootsector):
	@mkdir -p $(shell dirname $@)
	@cd bootloader/mbr && \
	cargo build --release -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem --target i386-mbr.json
	@objcopy -I elf32-i386 -O binary bootloader/mbr/target/i386-mbr/release/idos-mbr build/boot.bin


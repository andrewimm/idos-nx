fn main() {
    #[cfg(feature = "flat")]
    {
        let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        println!(
            "cargo:rustc-link-arg-bins=--script={}",
            crate_dir.join("link-script.ld").display(),
        );
    }
}

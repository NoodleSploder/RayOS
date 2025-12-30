fn main() {
    // Ensure relinking when the linker script changes.
    println!("cargo:rerun-if-changed=linker.ld");
}

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        // hlas.rc references the manifest and icon under assets/.
        embed_resource::compile("assets/hlas.rc", embed_resource::NONE);
    }
    if target.contains("msvc") && std::env::var_os("CARGO_FEATURE_VULKAN").is_some() {
        // ggml loads Vulkan through its own dynamic loader and falls back to
        // the CPU when that fails, but the import of vulkan-1.dll alone would
        // stop the exe from starting on a PC without a Vulkan driver.
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        // hlas.rc references the manifest and icon under assets/.
        embed_resource::compile("assets/hlas.rc", embed_resource::NONE);
    }
}

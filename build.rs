// Embeds the Windows manifest (per-monitor DPI awareness) and the app icon
// into the executable. On non-Windows hosts this is a no-op so the crate at
// least type-checks; the real build target is windows-msvc.
fn main() {
    #[cfg(windows)]
    {
        // hlas.rc references the manifest and icon under assets/.
        embed_resource::compile("assets/hlas.rc", embed_resource::NONE);
    }
}

fn main() {
    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/nsound.ico");
        resource.set("ProductName", "nSound");
        resource.set(
            "FileDescription",
            "nSound - pauses media when someone talks on Discord",
        );
        resource.set("LegalCopyright", "MIT licensed");

        if let Err(err) = resource.compile() {
            // Missing resource compiler must not break the build.
            println!("cargo:warning=cannot embed the Windows icon: {err}");
        }

        // The GNU linker drops the resource object from the static archive
        // because nothing references it, so pass the object file directly.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
            if let Ok(out_dir) = std::env::var("OUT_DIR") {
                let object = std::path::Path::new(&out_dir).join("resource.o");
                if object.exists() {
                    println!("cargo:rustc-link-arg={}", object.display());
                }
            }
        }
    }
}

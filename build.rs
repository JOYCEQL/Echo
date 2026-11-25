fn main() {
    println!("cargo:rerun-if-changed=packaging/Echo.ico");
    println!("cargo:rerun-if-changed=packaging/windows/Echo.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("packaging/Echo.ico")
            .set_manifest_file("packaging/windows/Echo.manifest")
            .set("ProductName", "Echo")
            .set("FileDescription", "Echo Notes")
            .set("OriginalFilename", "Echo.exe")
            .compile()
            .expect("compile Windows icon, version and application manifest");
    }
}

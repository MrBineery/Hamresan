fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Hamresan");
        res.set("FileDescription", "Local network file sharing");
        res.set("CompanyName", "Hamresan");
        res.set("LegalCopyright", "MIT OR Apache-2.0");
        if let Err(e) = res.compile() {
            eprintln!("warning: failed to compile Windows resources: {}", e);
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
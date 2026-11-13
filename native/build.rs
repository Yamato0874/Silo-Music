// Иконка и сведения о программе внутри exe.
fn main() {
    #[cfg(windows)]
    {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("../src-tauri/icons/icon.ico");
        r.set("ProductName", "Silo Player Native");
        r.set("FileDescription", "Silo Player (native)");
        r.set("CompanyName", "Silo");
        r.set("LegalCopyright", "© 2026 Silo");
        if let Err(e) = r.compile() { println!("cargo:warning=иконка не встроена: {e}"); }
    }
}

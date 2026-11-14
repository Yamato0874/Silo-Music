// Иконка и сведения о программе внутри exe.
fn main() {
    #[cfg(windows)]
    {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("assets/icon.ico");
        r.set("ProductName", "Silo Player Native");
        r.set("FileDescription", "Silo Player (native)");
        r.set("CompanyName", "Silo");
        r.set("LegalCopyright", "© 2026 Silo");
        r.set("OriginalFilename", "silo-native.exe");
        r.set("InternalName", "silo-native");
        r.set("Comments", "Открытый проект: github.com/Yamato0874/Silo-Music");
        // манифест: обычные права (без администратора), совместимость с Windows 10 и 11, чёткие DPI
        r.set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application></compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness><longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware></windowsSettings></application>
</assembly>"#);
        if let Err(e) = r.compile() { println!("cargo:warning=иконка не встроена: {e}"); }
    }
}

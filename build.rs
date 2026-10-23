fn main() {
    // Embed the application icon and version info into Orbit.exe.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Orbit");
        res.set("FileDescription", "Orbit — GravaStar configurator");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed icon: {e}");
        }
    }
}

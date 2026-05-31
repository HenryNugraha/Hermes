fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/assets/icon.ico");
        if let Err(err) = res.compile() {
            panic!("failed to compile windows resources: {err}");
        }
    }
}

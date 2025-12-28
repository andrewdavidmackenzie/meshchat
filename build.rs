extern crate winres;

pub fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/images/icon.ico"); // Set the path to your icon file
        res.compile().unwrap();
    }
    
    println!("cargo::rerun-if-changed=assets/fonts/icons.toml");
    iced_fontello::build("assets/fonts/icons.toml").expect("Build icons font");
}

pub fn main() {
    println!("cargo::rerun-if-changed=assets/fonts/icons.toml");
    iced_fontello::build("assets/fonts/icons.toml").expect("Build icons font");
}

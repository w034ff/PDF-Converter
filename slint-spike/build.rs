fn main() {
    for font in ["Regular", "Medium", "SemiBold"] {
        let path = format!("fonts/IBMPlexSansJP-{font}.ttf");
        if !std::path::Path::new(&path).exists() {
            panic!("{path} is missing: run scripts/fetch-font.sh first");
        }
    }
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("translations");
    slint_build::compile_with_config("ui/app.slint", config).expect("the .slint files compile");
}

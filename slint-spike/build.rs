fn main() {
    for f in ["fonts/IBMPlexSansJP-Regular.ttf", "fonts/IBMPlexSansJP-Bold.ttf"] {
        if !std::path::Path::new(f).exists() {
            panic!("{f} is missing: see README.md (Fonts) to extract IBM Plex Sans JP");
        }
    }
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("ui/app.slint compiles");
}

//! Where the bundled onnxruntime and layout model live (design §5.4, §8.5).

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// The onnxruntime library under the resource directory on Linux, one path
/// component per entry so that the path is built with the platform's
/// separator (Windows' loader does not take `/`). Joined with `/`, it is the
/// target in `tauri.linux.conf.json`.
const ONNXRUNTIME_RESOURCE_LINUX: [&str; 2] = ["onnxruntime", "libonnxruntime.so"];

/// As [`ONNXRUNTIME_RESOURCE_LINUX`], for `tauri.windows.conf.json`.
const ONNXRUNTIME_RESOURCE_WINDOWS: [&str; 2] = ["onnxruntime", "onnxruntime.dll"];

const ONNXRUNTIME_RESOURCE: [&str; 2] = if cfg!(windows) {
    ONNXRUNTIME_RESOURCE_WINDOWS
} else {
    ONNXRUNTIME_RESOURCE_LINUX
};

/// The layout model under the resource directory, as
/// [`ONNXRUNTIME_RESOURCE_LINUX`]; the same target on both platforms.
const LAYOUT_MODEL_RESOURCE: [&str; 2] = ["layout-model", "PP-DocLayout_plus-L-int8.onnx"];

/// The bundled onnxruntime library file, passed to each worker. Whether it
/// exists is left to the worker's first `Detect` (design §5.4).
///
/// # Errors
///
/// Returns Tauri's error when the resource directory cannot be resolved.
pub fn onnxruntime_file(app: &AppHandle) -> tauri::Result<PathBuf> {
    resource(app, ONNXRUNTIME_RESOURCE)
}

/// The bundled layout model file, passed to each worker as
/// [`onnxruntime_file`] is.
///
/// # Errors
///
/// Returns Tauri's error when the resource directory cannot be resolved.
pub fn model_file(app: &AppHandle) -> tauri::Result<PathBuf> {
    resource(app, LAYOUT_MODEL_RESOURCE)
}

fn resource(app: &AppHandle, components: [&str; 2]) -> tauri::Result<PathBuf> {
    Ok(app
        .path()
        .resource_dir()?
        .join(components.iter().collect::<PathBuf>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The targets of `bundle.resources` in `tauri.<platform>.conf.json`.
    fn bundled_targets(platform: &str) -> Vec<String> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tauri.{platform}.conf.json"));
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        config["bundle"]["resources"]
            .as_object()
            .unwrap()
            .values()
            .map(|target| target.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_files_are_where_the_bundles_put_them() {
        for (platform, onnxruntime) in [
            ("linux", ONNXRUNTIME_RESOURCE_LINUX),
            ("windows", ONNXRUNTIME_RESOURCE_WINDOWS),
        ] {
            let targets = bundled_targets(platform);
            for resource in [onnxruntime, LAYOUT_MODEL_RESOURCE] {
                let target = resource.join("/");
                assert!(
                    targets.contains(&target),
                    "{platform}: {target} in {targets:?}"
                );
            }
        }
    }
}

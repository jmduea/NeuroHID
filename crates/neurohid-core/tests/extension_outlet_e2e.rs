//! E2E test: load the example outlet extension and assert creation.
//!
//! Requires the example outlet to be built first (e.g. `cargo build --workspace`).
//! Copies the built dylib and a manifest into a temp dir, scans the registry,
//! and asserts create_outlet returns the extension by name.

use neurohid_core::extension_registry::ExtensionRegistry;
use neurohid_core::tasks::create_outlet;
use neurohid_types::config::OutletConfig;
use std::env;
use std::fs;
use std::path::Path;

fn find_example_lib() -> Option<std::path::PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    let ws_root = Path::new(&manifest_dir).ancestors().nth(2)?;
    let target_dir = env::var("CARGO_TARGET_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| ws_root.join("target"));
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let lib_name = {
        #[cfg(windows)]
        {
            format!("neurohid_outlet_example{}", env::consts::DLL_EXTENSION)
        }
        #[cfg(not(windows))]
        {
            format!("libneurohid_outlet_example{}", env::consts::DLL_EXTENSION)
        }
    };
    let lib_path = target_dir.join(profile).join(&lib_name);
    if lib_path.exists() {
        Some(lib_path)
    } else {
        None
    }
}

#[test]
fn example_outlet_loads_and_creates_by_name() {
    let lib_path = match find_example_lib() {
        Some(p) => p,
        None => {
            eprintln!(
                "skip: neurohid-outlet-example not built; run `cargo build -p neurohid-outlet-example` first"
            );
            return;
        }
    };

    let temp_parent = env::temp_dir().join("neurohid_ext_e2e");
    let _ = fs::remove_dir_all(&temp_parent);
    let ext_dir = temp_parent.join("neurohid-outlet-example");
    fs::create_dir_all(&ext_dir).expect("create ext dir");

    let lib_name = lib_path.file_name().unwrap().to_str().unwrap();
    let dest_lib = ext_dir.join(lib_name);
    fs::copy(&lib_path, &dest_lib).expect("copy dylib");

    let manifest = format!(
        r#"{{ "name": "neurohid-outlet-example", "kind": "outlet", "library": "{}" }}"#,
        lib_name
    );
    fs::write(ext_dir.join("manifest.json"), manifest).expect("write manifest");

    let mut reg = ExtensionRegistry::new(vec![temp_parent.clone()]);
    reg.scan().expect("scan");

    let config = OutletConfig {
        enabled: true,
        extension_name: Some("neurohid-outlet-example".to_string()),
        ..OutletConfig::default()
    };
    let (outlet, name) =
        create_outlet(config, None, None, None, None, Some(&reg)).expect("create_outlet");
    assert_eq!(name, "neurohid-outlet-example");
    drop(outlet);

    let _ = fs::remove_dir_all(&temp_parent);
}

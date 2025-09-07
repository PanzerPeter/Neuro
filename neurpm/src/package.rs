//! Package utilities

use crate::{PackageManifest, NeuropmResult};
use std::path::Path;

pub fn load_manifest(path: &Path) -> NeuropmResult<PackageManifest> {
    let _ = path;
    Err(crate::NeuropmError::PackageNotFound {
        name: "placeholder".to_string(),
    })
}

pub fn save_manifest(manifest: &PackageManifest, path: &Path) -> NeuropmResult<()> {
    let _ = (manifest, path);
    Ok(())
}
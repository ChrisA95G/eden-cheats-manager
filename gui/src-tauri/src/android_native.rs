/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod jni;
mod library_discovery;
mod package_discovery;
mod saf;

pub use self::library_discovery::*;
pub use self::package_discovery::*;
pub use self::saf::*;

#[cfg(target_os = "android")]
pub(crate) fn app_update_download_path() -> Result<std::path::PathBuf, String> {
    jni::parse_saf_response(jni::jni_noarg_string_call("getAppUpdateDownloadPath")?)
        .map(std::path::PathBuf::from)
}

#[cfg(target_os = "android")]
pub(crate) fn install_app_update(version: &str) -> Result<String, String> {
    jni::parse_saf_response(jni::jni_string_call("installAppUpdate", version)?)
}

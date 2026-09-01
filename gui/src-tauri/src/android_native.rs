/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod jni;
mod package_discovery;
mod saf;

pub use self::package_discovery::*;
pub use self::saf::*;

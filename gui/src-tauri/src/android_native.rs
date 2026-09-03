/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod jni;
mod library_discovery;
mod package_discovery;
mod saf;

pub use self::library_discovery::*;
pub use self::package_discovery::*;
pub use self::saf::*;

/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod discovery;
mod jni;
mod package_discovery;
mod saf;

pub use self::discovery::*;
pub use self::package_discovery::*;
pub use self::saf::*;

/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod discovery;
mod jni;
mod saf;

pub use self::discovery::*;
pub use self::saf::*;

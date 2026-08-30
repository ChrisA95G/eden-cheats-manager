/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
mod build_id_scan;
mod jni;
mod saf;

pub use self::build_id_scan::*;
pub use self::saf::*;

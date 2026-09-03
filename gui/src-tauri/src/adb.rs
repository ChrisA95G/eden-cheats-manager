use regex::Regex;
use std::sync::OnceLock;

pub(crate) const EDEN_VIRTUAL_DIRS: &[&str] = &["SDMC", "UserNAND", "SysNAND"];

static LOADER_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();

pub(crate) fn loader_build_id_re() -> &'static Regex {
    LOADER_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64}),\s*name=main").unwrap()
    })
}

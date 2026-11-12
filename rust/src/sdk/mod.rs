//! SDK installation and verification.

pub mod install;
pub mod verify;

pub use install::{
    install_for_project, ANDROID_SDK_ASSET, ANDROID_SDK_DIR, STATUS_INSTALL_FAILED,
    STATUS_NOT_APPLICABLE, STATUS_SKIPPED,
};
pub use verify::{sha256_file, verify_android_artifact};

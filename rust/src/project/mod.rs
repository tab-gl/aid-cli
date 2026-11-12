//! Project layer.

pub mod detect;

pub use detect::{
    declared_dependency_version, detect_project, installed_sdk_version, project_name, Project,
    SdkMode,
};

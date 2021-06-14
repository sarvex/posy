mod core_metadata;
mod extra;
mod package_name;
mod requirement;
mod rfc822ish;
mod wheel_metadata;
mod wheel_name;
mod version;
mod reqparse;
mod specifier;

// All this stuff is also re-exported from crate::prelude::*

pub use self::core_metadata::CoreMetadata;
pub use self::extra::Extra;
pub use self::package_name::PackageName;
pub use self::requirement::{
    marker, ParseExtra, Requirement,
};
pub use self::wheel_metadata::WheelMetadata;
pub use self::wheel_name::WheelName;
pub use self::version::Version;
pub use self::specifier::{CompareOp, Specifier, Specifiers};

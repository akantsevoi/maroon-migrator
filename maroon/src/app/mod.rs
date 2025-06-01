pub mod app;
pub mod app_interface;
pub mod app_params;

#[cfg(test)]
mod app_tests_multi; // test several apps as a black box
#[cfg(test)]
mod app_tests_single; // test app as a black box

pub use app::App;
pub use app_interface::{CurrentOffsets, Request, Response};
pub use app_params::Params;

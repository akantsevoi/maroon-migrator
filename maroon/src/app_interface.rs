use common::range_key::{KeyOffset, KeyRange};
use derive_more::Display;
use std::collections::HashMap;

#[derive(Display)]
pub enum Request {
    GetState,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    State(CurrentOffsets),
}

#[derive(Debug, PartialEq, Eq)]
pub struct CurrentOffsets {
    pub self_offsets: HashMap<KeyRange, KeyOffset>,
    pub consensus_offset: HashMap<KeyRange, KeyOffset>,
}

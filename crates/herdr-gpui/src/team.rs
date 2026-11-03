//! Native Team section. This view receives prepared data and emits intents;
//! its owner performs storage and daemon work away from GPUI rendering.

mod bridge;
pub(crate) mod model;
mod view;

pub(crate) use bridge::Bridge;
pub(crate) use view::{TeamAction, TeamView};

pub(crate) const ACTION_QUEUE_CAPACITY: usize = 64;

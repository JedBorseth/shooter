//! Marks a body that controller aim assist can lock on to.

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;
use shooter_sim::look::AssistTarget;

use crate::convert::to_sim;

/// Scene group that holds every [`AimAssistTarget`].
pub const AIM_ASSIST_GROUP: &str = "aim_assist_targets";

/// Add as a child of a target, with its origin at the target's feet. Describes
/// the body as an upright capsule. Disable it while the target is dead.
#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub struct AimAssistTarget {
    base: Base<Node3D>,

    /// Capsule radius in metres.
    #[export]
    #[init(val = 0.3)]
    radius: f32,
    /// Capsule height in metres, from the origin up.
    #[export]
    #[init(val = 1.75)]
    height: f32,
    #[export]
    #[init(val = true)]
    enabled: bool,
}

#[godot_api]
impl INode3D for AimAssistTarget {
    fn enter_tree(&mut self) {
        self.base_mut().add_to_group(AIM_ASSIST_GROUP);
    }
}

impl AimAssistTarget {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_active(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// The capsule in world space, keyed by this node's instance ID.
    pub fn capsule(&self) -> AssistTarget {
        let transform = self.base().get_global_transform();
        let up = transform.basis.col_b().normalized();
        let origin = transform.origin;
        let radius = self.radius.min(self.height * 0.5);
        AssistTarget {
            id: self.base().instance_id().to_i64() as u64,
            bottom: to_sim(origin + up * radius),
            top: to_sim(origin + up * (self.height - radius)),
            radius,
        }
    }
}

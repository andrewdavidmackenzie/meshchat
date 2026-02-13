pub mod subscription;

pub const MESHCORE_SERVICE_UUID: Uuid = Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);

use crate::{MCPosition, MCUser};
use meshcore_rs::events::SelfInfo;
use uuid::Uuid;

/// Conversions between [SelfIno] and MeshChat [MCUser]
impl From<&SelfInfo> for MCUser {
    fn from(self_info: &SelfInfo) -> Self {
        MCUser {
            long_name: self_info.name.clone(),
            short_name: self_info.name.clone(),
            ..Default::default()
        }
    }
}

/// Conversions between [SelfIno] and MeshChat [MCPosition]
impl From<&SelfInfo> for MCPosition {
    fn from(self_info: &SelfInfo) -> Self {
        MCPosition {
            latitude: self_info.adv_lat as f64 / 1_000_000.0,
            longitude: self_info.adv_lon as f64 / 1_000_000.0,
            ..Default::default()
        }
    }
}

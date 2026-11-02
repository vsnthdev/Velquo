use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;

impl KdeConnectDaemon {
    pub fn read_macos_battery_status() -> Option<(i32, bool)> {
        use objc2::msg_send;
        use objc2_foundation::{NSDictionary, NSString};
        use std::ffi::c_void;

        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOPSCopyPowerSourcesInfo() -> *const c_void;
            fn IOPSCopyPowerSourcesList(blob: *const c_void) -> *const c_void;
            fn IOPSGetPowerSourceDescription(
                blob: *const c_void,
                ps: *const c_void,
            ) -> *const c_void;
        }

        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFArrayGetCount(the_array: *const c_void) -> isize;
            fn CFArrayGetValueAtIndex(the_array: *const c_void, idx: isize) -> *const c_void;
            fn CFRelease(cf: *const c_void);
        }

        unsafe {
            let blob = IOPSCopyPowerSourcesInfo();
            if blob.is_null() {
                return None;
            }
            let list = IOPSCopyPowerSourcesList(blob);
            if list.is_null() {
                CFRelease(blob);
                return None;
            }

            let count = CFArrayGetCount(list);
            let mut result: Option<(i32, bool)> = None;

            let key_cur = NSString::from_str("Current Capacity");
            let key_max = NSString::from_str("Max Capacity");
            let key_charging = NSString::from_str("Is Charging");
            let key_charged = NSString::from_str("Is Charged");
            let key_state = NSString::from_str("Power Source State");

            for idx in 0..count {
                let ps = CFArrayGetValueAtIndex(list, idx);
                if ps.is_null() {
                    continue;
                }
                let desc_ptr = IOPSGetPowerSourceDescription(blob, ps);
                if desc_ptr.is_null() {
                    continue;
                }
                let dict = &*(desc_ptr as *const NSDictionary);

                let Some(cur_obj) = dict.objectForKey(&key_cur) else {
                    continue;
                };
                let cur_cap: i32 = msg_send![&*cur_obj, intValue];
                let max_cap: i32 = dict
                    .objectForKey(&key_max)
                    .map(|o| msg_send![&*o, intValue])
                    .unwrap_or(100);

                let charge = if max_cap > 0 {
                    ((cur_cap * 100) / max_cap).clamp(0, 100)
                } else {
                    cur_cap.clamp(0, 100)
                };

                let is_charging: bool = dict
                    .objectForKey(&key_charging)
                    .map(|o| msg_send![&*o, boolValue])
                    .unwrap_or(false);
                let is_charged: bool = dict
                    .objectForKey(&key_charged)
                    .map(|o| msg_send![&*o, boolValue])
                    .unwrap_or(false);
                let on_ac = dict
                    .objectForKey(&key_state)
                    .and_then(|o| o.downcast::<NSString>().ok())
                    .map(|s| s.to_string() == "AC Power")
                    .unwrap_or(false);

                result = Some((charge, is_charging || is_charged || on_ac));
                break;
            }

            CFRelease(list);
            CFRelease(blob);
            result
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[cfg(not(target_os = "macos"))]
    pub fn read_macos_battery_status() -> Option<(i32, bool)> {
        None
    }

    pub fn send_host_battery_to_device(&self, device_id: &str) {
        if let Some((charge, is_charging)) = Self::read_macos_battery_status() {
            let pkt = NetworkPacket::battery_status(charge, is_charging);
            if self.send_packet_to_device(device_id, &pkt).is_ok() {
                log::info!(
                    "[battery-out] Sent Mac battery status to {}: {}% (charging={})",
                    device_id,
                    charge,
                    is_charging
                );
            }
        }
    }

    pub fn broadcast_host_battery(&self, charge: i32, is_charging: bool) {
        let pkt = NetworkPacket::battery_status(charge, is_charging);
        let paired_online_ids: Vec<String> = {
            let devices = self.devices.lock().unwrap();
            devices
                .values()
                .filter(|d| d.view.reachable && d.view.pair_state == PairState::Paired)
                .map(|d| d.view.device_id.clone())
                .collect()
        };
        for id in paired_online_ids {
            let _ = self.send_packet_to_device(&id, &pkt);
        }
    }
}

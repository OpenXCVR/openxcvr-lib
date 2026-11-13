#![cfg(feature = "hid")]

use openxcvr_transports::hid::NativeHid;

#[test]
fn held_info_is_available_on_the_existing_handle() {
    // Type-check the public method without acquiring or opening any device.
    fn inspect(held: &NativeHid) {
        let _ = held.held_info();
    }
    let _: fn(&NativeHid) = inspect;
}

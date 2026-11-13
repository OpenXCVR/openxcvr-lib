//! Synthetic adapter profiles shared by the transport and host-state test suites.
//!
//! Every transport and host-state test includes this module with `mod support;`
//! and runs its checks against both profiles, so adapter neutrality is tested
//! rather than asserted (plan task 2; goals G1 and G4).
//!
//! The profiles are declarations: identity values plus the transport-shaped data
//! a test needs to build a fake device or a fake sysfs tree. They are compile-time
//! constants written by hand for this suite, not captures from a device, and they
//! carry no capability beyond identity and no adapter-dependent behaviour:
//! nothing here branches on which profile it was handed, and no member can drive a
//! pin, a supply or a restart. `crates/openxcvr-transports/tests/profiles.rs`
//! holds the module to those properties.

/// One field of a [`Profile`], so a profile can be described and compared by name
/// without reflection or allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// A numeric identity value: a vendor or product ID.
    Id(u16),
    /// A name: a kernel driver name or a sysfs class directory.
    Name(&'static str),
}

/// The transport-shaped part of a profile: what a test derives from an adapter
/// once its identity is set aside. [`Profile::shape`] builds it the same way for
/// every profile, so a difference here would be adapter-specific behaviour rather
/// than adapter data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterShape {
    /// sysfs class directory a hidraw node for this adapter appears under.
    pub hidraw_class: &'static str,
    /// sysfs class directory the adapter's i2c interface appears under.
    pub i2c_adapter_class: &'static str,
}

/// An adapter as the tests describe it: the identity a caller supplies to a
/// transport, plus the shapes tests build fake devices and fake sysfs trees from.
///
/// Field contract: [`Profile::fields`] enumerates every field by name and
/// [`FIELD_NAMES`] is the manifest a test compares that enumeration against. Two
/// profiles may differ only in [`DIFFERING_FIELD_NAMES`] — vendor ID, product ID
/// and kernel driver name — never in an [`AdapterShape`], and never in a
/// capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// USB vendor ID, as the caller supplies it to a transport.
    pub vendor_id: u16,
    /// USB product ID, as the caller supplies it to a transport.
    pub product_id: u16,
    /// Kernel driver that may claim the HID interface without exposing a node.
    pub kernel_driver: &'static str,
    /// sysfs class directory a hidraw node for this adapter appears under.
    pub hidraw_class: &'static str,
    /// sysfs class directory the adapter's i2c interface appears under.
    pub i2c_adapter_class: &'static str,
}

/// Every field of [`Profile`], in declaration order. Kept beside the struct so a
/// test can prove the manifest still describes the whole profile.
pub const FIELD_NAMES: [&str; 5] = [
    "vendor_id",
    "product_id",
    "kernel_driver",
    "hidraw_class",
    "i2c_adapter_class",
];

/// The fields two profiles are allowed to differ in: the adapter's identity.
pub const DIFFERING_FIELD_NAMES: [&str; 3] = ["vendor_id", "product_id", "kernel_driver"];

impl Profile {
    /// Every field, by name, in declaration order.
    pub fn fields(&self) -> [(&'static str, Field); 5] {
        [
            ("vendor_id", Field::Id(self.vendor_id)),
            ("product_id", Field::Id(self.product_id)),
            ("kernel_driver", Field::Name(self.kernel_driver)),
            ("hidraw_class", Field::Name(self.hidraw_class)),
            ("i2c_adapter_class", Field::Name(self.i2c_adapter_class)),
        ]
    }

    /// The transport-shaped data, identity excluded.
    pub fn shape(&self) -> AdapterShape {
        AdapterShape {
            hidraw_class: self.hidraw_class,
            i2c_adapter_class: self.i2c_adapter_class,
        }
    }
}

/// The MCP2221A bridge: USB `04d8:00dd`, whose HID interface the kernel's
/// `hid_mcp2221` driver claims.
pub const MCP2221A: Profile = Profile {
    vendor_id: 0x04d8,
    product_id: 0x00dd,
    kernel_driver: "hid_mcp2221",
    hidraw_class: "hidraw",
    i2c_adapter_class: "i2c-dev",
};

/// A synthetic non-MCP2221A bridge, with other IDs and another kernel driver name
/// (the second profile Q7 asks for). Its IDs are fixture values that belong to no
/// device and its driver name is one no kernel provides, so nothing here is a
/// capture from real hardware.
pub const SYNTHETIC_NON_MCP2221A: Profile = Profile {
    vendor_id: 0xdead,
    product_id: 0xbeef,
    kernel_driver: "hid_synthetic_bridge",
    hidraw_class: "hidraw",
    i2c_adapter_class: "i2c-dev",
};

/// Both profiles, in the order the suite runs them.
pub const PROFILES: [Profile; 2] = [MCP2221A, SYNTHETIC_NON_MCP2221A];

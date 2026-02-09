//! # LSL (Lab Streaming Layer) Device Backend
//!
//! Consumes any LSL stream available on the local network. Device-specific
//! software (e.g., emotiv-cortex-cli, MuseLSL, OpenBCI GUI) pushes data
//! into LSL; this adapter pulls it into the NeuroHID pipeline.

pub(crate) mod device;
mod provider;

pub use device::LslDevice;
pub use provider::LslProvider;

/// Configure liblsl to avoid multicast warnings on Windows.
///
/// liblsl tries to bind multicast responders on 224.0.0.1 ("All Hosts"), which
/// fails on Windows interfaces that don't support it (Hyper-V, VPN, WSL2).
/// This writes a config file restricting liblsl to the standard LSL multicast
/// group and points `LSLAPICFG` at it.
pub(crate) fn configure_lsl() {
    let cfg_content = "\
[multicast]
listen_address = 239.255.172.215
";

    let path = std::env::temp_dir().join("neurohid_lsl_api.cfg");
    if std::fs::write(&path, cfg_content).is_ok() {
        // SAFETY: called once at startup before any LSL threads are spawned.
        unsafe {
            std::env::set_var("LSLAPICFG", &path);
        }
    }
}

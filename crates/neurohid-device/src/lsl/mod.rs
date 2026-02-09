//! # LSL (Lab Streaming Layer) Device Backend
//!
//! Consumes any LSL stream available on the local network. Device-specific
//! software (e.g., emotiv-cortex-cli, MuseLSL, OpenBCI GUI) pushes data
//! into LSL; this adapter pulls it into the NeuroHID pipeline.

pub(crate) mod device;
mod provider;

pub use device::LslDevice;
pub use provider::LslProvider;

/// Prepare liblsl for use.
///
/// Currently a no-op — we rely on liblsl's built-in defaults (ResolveScope =
/// site, standard multicast address pools). This is the same configuration
/// LabRecorder uses successfully.
///
/// Previous versions wrote a custom `LSLAPICFG` config file that set
/// `resolve_scope = link` and `listen_address = 239.255.172.215`. This was
/// broken for two reasons:
///
/// 1. `listen_address` controls the TCP bind address, NOT multicast discovery
///    addresses. The correct setting for restricting multicast would be
///    `AddressesOverride`.
/// 2. `resolve_scope = link` limits discovery to LinkAddresses only
///    (255.255.255.255, 224.0.0.183) — excluding the site-scoped
///    239.255.172.215 that most outlets announce on.
///
/// Any warnings liblsl emits about Hyper-V / VPN adapters failing to bind
/// multicast are harmless and suppressed by liblsl's default log level.
pub(crate) fn configure_lsl() {
    // Intentionally empty — use liblsl defaults.
}

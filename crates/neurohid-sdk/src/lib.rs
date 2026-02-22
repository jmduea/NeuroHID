//! # NeuroHID SDK
//!
//! A feature-gated facade crate that provides access to NeuroHID's internal
//! libraries. Enable only the features you need to keep compile times fast
//! and dependency trees minimal.
//!
//! ## Features
//!
//! | Feature | Crate | Description |
//! |---------|-------|-------------|
//! | `types` (default) | `neurohid-types` | Core type definitions (signals, actions, devices) |
//! | `signal` | `neurohid-signal` | Real-time biosignal processing pipeline |
//! | `device` | `neurohid-device` | Device abstraction layer for biosensors |
//! | `device-lsl` | `neurohid-device` + LSL | Device layer with Lab Streaming Layer support |
//! | `platform` | `neurohid-platform` | Cross-platform HID emulation |
//! | `storage` | `neurohid-storage` | Secure profile and config storage |
//! | `ipc` | `neurohid-ipc` | IPC layer for Rust↔Python communication |
//! | `calibration` | `neurohid-calibration` | Calibration games and wizard |
//! | `runtime` | `neurohid-core` | Managed runtime/service APIs |
//! | `hub` | `neurohid-hub` | Hub GUI library |
//! | `full` | All of the above | Everything enabled |
//!
//! ## Quick Start
//!
//! ```toml
//! [dependencies]
//! neurohid-sdk = { version = "0.1", features = ["device", "signal"] }
//! ```
//!
//! ```rust,ignore
//! use neurohid_sdk::types;
//! use neurohid_sdk::device;
//! use neurohid_sdk::signal;
//! ```
//!
//! For managed runtime embedding, see:
//! - `examples/embedded_runtime.rs`
//! - `README.md` runtime embedding section

#[cfg(feature = "types")]
pub use neurohid_types as types;

#[cfg(feature = "signal")]
pub use neurohid_signal as signal;

#[cfg(feature = "device")]
pub mod device;

#[cfg(feature = "platform")]
pub use neurohid_platform as platform;

#[cfg(feature = "storage")]
pub mod config;

#[cfg(feature = "storage")]
pub use neurohid_storage as storage;

#[cfg(feature = "ipc")]
pub use neurohid_ipc as ipc;

#[cfg(feature = "calibration")]
pub use neurohid_calibration as calibration;

#[cfg(feature = "runtime")]
pub use neurohid_core as runtime;

#[cfg(feature = "hub")]
pub use neuroide_hub as hub;

#[cfg(test)]
mod tests {
    // ───────────── types (default feature) ─────────────

    #[cfg(feature = "types")]
    mod types_tests {
        use crate::types;

        #[test]
        fn system_config_default() {
            let cfg = types::config::SystemConfig::default();
            // SystemConfig exists and is Default
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn device_id_constructible() {
            let id = types::DeviceId::new("mock-001");
            assert_eq!(id.0, "mock-001");
        }

        #[test]
        fn sample_constructible() {
            let sample = types::signal::Sample::new(vec![1.0, 2.0, 3.0]);
            assert_eq!(sample.channel_count(), 3);
        }

        #[test]
        fn feature_vector_constructible() {
            let fv = types::signal::FeatureVector::new(vec![0.5; 4]);
            assert_eq!(fv.values.len(), 4);
        }

        #[test]
        fn action_space_default() {
            let space = types::action::ActionSpace::default();
            let _ = format!("{space:?}");
        }

        #[test]
        fn key_enum_variants_exist() {
            let _up = types::Key::ArrowUp;
            let _down = types::Key::ArrowDown;
            let _left = types::Key::ArrowLeft;
            let _right = types::Key::ArrowRight;
        }

        #[test]
        fn mouse_button_variants_exist() {
            let _left = types::MouseButton::Left;
            let _right = types::MouseButton::Right;
        }

        #[test]
        fn connection_state_variants_exist() {
            let _disconnected = types::ConnectionState::Disconnected;
            let _connected = types::ConnectionState::Connected;
        }

        #[test]
        fn control_request_constructible() {
            let req = types::ControlRequest::new(types::ControlCommand::Snapshot);
            let _ = format!("{req:?}");
        }

        #[test]
        fn now_micros_returns_positive() {
            let ts = types::now_micros();
            assert!(ts > 0);
        }

        #[test]
        fn channel_id_constructible() {
            let ch = types::signal::ChannelId::new("AF3");
            assert_eq!(ch.0, "AF3");
        }

        #[test]
        fn reward_types_exist() {
            let _cfg = types::reward::ErrPConfig::default();
            let _quality = types::reward::SignalQuality::Good;
            let _ = std::any::type_name::<types::reward::RewardSignal>();
        }

        #[test]
        fn profile_id_default() {
            let id = types::ProfileId::default();
            let _ = format!("{id:?}");
        }
    }

    // ───────────── signal ─────────────

    #[cfg(feature = "signal")]
    mod signal_tests {
        use crate::signal;

        #[test]
        fn buffer_config_default() {
            let cfg = signal::buffer::BufferConfig::default();
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn sample_buffer_constructible() {
            let _buf = signal::buffer::SampleBuffer::new(signal::buffer::BufferConfig::default());
        }

        #[test]
        fn filter_type_variants_exist() {
            let _notch = signal::filter::FilterType::Notch {
                center_hz: 60.0,
                q_factor: 30.0,
            };
        }

        #[test]
        fn filter_chain_eeg_default() {
            let _chain = signal::filter::FilterChain::eeg_default(128.0, 5, 60.0);
        }

        #[test]
        fn pipeline_config_default() {
            let cfg = signal::pipeline::PipelineConfig::default();
            assert!(cfg.artifact_threshold_uv > 0.0);
        }

        #[test]
        fn module_reexports_accessible() {
            // Verify public modules are accessible through the SDK
            let _ = std::any::type_name::<signal::buffer::SampleBuffer>();
            let _ = std::any::type_name::<signal::filter::FilterChain>();
            let _ = std::any::type_name::<signal::features::FeatureExtractor>();
            let _ = std::any::type_name::<signal::pipeline::SignalPipeline>();
        }
    }

    // ───────────── device (BrainFlow synthetic as non-hardware path) ─────────────

    #[cfg(all(feature = "device", feature = "device-brainflow"))]
    mod device_tests {
        use crate::device;

        #[test]
        fn brainflow_config_default() {
            let cfg = crate::types::config::BrainFlowConfig::default();
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn brainflow_provider_constructible() {
            let _provider =
                device::BrainFlowProvider::new(crate::types::config::BrainFlowConfig::default());
        }

        #[test]
        fn trait_types_accessible() {
            let _ = std::any::type_name::<dyn device::traits::Device>();
            let _ = std::any::type_name::<dyn device::traits::DeviceProvider>();
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<device::mock::MockDevice>();
            let _ = std::any::type_name::<device::serial::SerialProvider>();
        }
    }

    // ───────────── platform ─────────────

    #[cfg(feature = "platform")]
    mod platform_tests {
        use crate::platform;

        #[test]
        fn platform_config_default() {
            let cfg = platform::traits::PlatformConfig::default();
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn trait_type_accessible() {
            let _ = std::any::type_name::<dyn platform::traits::Platform>();
        }

        #[test]
        fn permission_hint_type_exists() {
            let _ = std::any::type_name::<platform::traits::PermissionHint>();
        }
    }

    // ───────────── storage ─────────────

    #[cfg(feature = "storage")]
    mod storage_tests {
        use crate::storage;

        #[test]
        fn data_paths_constructible() {
            // Use a temp dir to avoid touching real paths
            let tmp = std::env::temp_dir().join("neurohid-sdk-test");
            let paths = storage::paths::DataPaths::new(Some(tmp));
            assert!(paths.is_ok());
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<storage::config::ConfigStore>();
            let _ = std::any::type_name::<storage::profile::ProfileStore>();
            let _ = std::any::type_name::<storage::secure::SecureStorage>();
        }
    }

    #[cfg(feature = "storage")]
    mod config_tests {
        use crate::config;

        #[tokio::test]
        async fn config_load_save_public_api() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("config.toml");
            let config_path = Some(path.clone());

            let cfg = neurohid_types::config::SystemConfig::default();
            config::save(config_path.clone(), &cfg).await.unwrap();
            let loaded = config::load(config_path).await.unwrap();
            assert_eq!(
                loaded.format_version,
                neurohid_types::config::CURRENT_CONFIG_FORMAT_VERSION
            );
        }
    }

    // ───────────── ipc ─────────────

    #[cfg(feature = "ipc")]
    mod ipc_tests {
        use crate::ipc;

        #[test]
        fn ipc_config_default() {
            let cfg = ipc::protocol::IpcConfig::default();
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn broker_config_default() {
            let cfg = ipc::protocol::BrokerConfig::default();
            let _ = format!("{cfg:?}");
        }

        #[test]
        fn ipc_transport_default() {
            let t = ipc::protocol::IpcTransport::default();
            let _ = format!("{t:?}");
        }

        #[test]
        fn default_endpoints_not_empty() {
            assert!(!ipc::protocol::default_ipc_endpoint().is_empty());
            assert!(!ipc::protocol::default_runtime_endpoint().is_empty());
            assert!(!ipc::protocol::default_control_endpoint().is_empty());
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<ipc::broker::IpcBroker>();
            let _ = std::any::type_name::<ipc::server::IpcServer>();
            let _ = std::any::type_name::<ipc::client::IpcClient>();
        }
    }

    // ───────────── calibration ─────────────

    #[cfg(feature = "calibration")]
    mod calibration_tests {
        use crate::calibration;

        #[test]
        fn wizard_state_default() {
            let _state = calibration::wizard::WizardState::default();
        }

        #[test]
        fn wizard_state_new() {
            let _state = calibration::wizard::WizardState::new();
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<calibration::panel::CalibrationPanel>();
            let _ = std::any::type_name::<calibration::games::GridMazeGame>();
            let _ = std::any::type_name::<calibration::games::TargetTrackingGame>();
        }
    }

    // ───────────── runtime ─────────────

    #[cfg(feature = "runtime")]
    mod runtime_tests {
        use crate::runtime;

        #[test]
        fn runtime_snapshot_default() {
            let snap = runtime::runtime::RuntimeSnapshot::default();
            let _ = format!("{snap:?}");
        }

        #[test]
        fn runtime_builder_constructible() {
            let cfg = neurohid_types::config::SystemConfig::default();
            let _builder = runtime::runtime::RuntimeBuilder::new(cfg);
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<runtime::runtime::RuntimeHandle>();
            let _ = std::any::type_name::<runtime::service::NeuroHidService>();
        }
    }

    // ───────────── hub ─────────────

    #[cfg(feature = "hub")]
    mod hub_tests {
        use crate::hub;

        #[test]
        fn workbench_state_default() {
            let state = hub::workbench::WorkbenchState::default();
            let _ = format!("{state:?}");
        }

        #[test]
        fn layout_manager_default() {
            let _lm = hub::layout::LayoutManager::default();
        }

        #[test]
        fn activity_lane_variants_exist() {
            let _ops = hub::workbench::ActivityLane::Ops;
            let _ = format!("{_ops:?}");
        }

        #[test]
        fn module_reexports_accessible() {
            let _ = std::any::type_name::<hub::app::HubApp>();
            let _ = std::any::type_name::<hub::data_bus::DataBus>();
            let _ = std::any::type_name::<hub::service_manager::ServiceManager>();
        }
    }

    // ───────────── cross-feature coherence ─────────────

    #[cfg(all(feature = "types", feature = "device", feature = "device-brainflow"))]
    #[test]
    fn device_id_shared_across_crates() {
        // DeviceId from types is the same type used in device crate
        let id = crate::types::DeviceId::new("coherence-test");
        let cfg = crate::types::config::BrainFlowConfig::default();
        let _provider = crate::device::BrainFlowProvider::new(cfg);
        let _ = format!("{id:?}");
    }

    #[cfg(all(feature = "types", feature = "signal"))]
    #[test]
    fn sample_type_shared_across_crates() {
        // Sample from types is the type consumed by signal pipeline
        let sample = crate::types::signal::Sample::new(vec![1.0, 2.0]);
        let buf = crate::signal::buffer::SampleBuffer::new(
            crate::signal::buffer::BufferConfig::default(),
        );
        let _ = (sample, buf);
    }

    // ───────────── device API (runtime + device) ─────────────

    #[cfg(all(feature = "runtime", feature = "device", feature = "device-brainflow"))]
    mod device_api_tests {
        use crate::device;

        #[tokio::test]
        async fn list_streams_discovery_returns_brainflow_synthetic_stream() {
            let provider =
                device::BrainFlowProvider::new(crate::types::config::BrainFlowConfig::default());
            let streams = device::list_streams_discovery(&provider).await.unwrap();
            assert_eq!(streams.len(), 1);
            assert!(streams[0].id.starts_with("brainflow::"));
            assert!(!streams[0].name.is_empty());
        }
    }
}

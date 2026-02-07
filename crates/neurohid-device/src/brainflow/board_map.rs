//! # Board ID Mapping
//!
//! Bidirectional mapping between BrainFlow's `BoardIds` enum and NeuroHID's
//! `DeviceType` enum. This is the single source of truth for which BrainFlow
//! boards correspond to which NeuroHID device types.
//!
//! ## Adding a New Board
//!
//! 1. Add the mapping to `board_id_to_device_type` and `device_type_to_board_id`
//! 2. If the board uses a DeviceType that doesn't exist yet, add it to
//!    `neurohid-types/src/device.rs` first
//! 3. The rest of the adapter (provider, device, stream) works generically

use brainflow::BoardIds;
use neurohid_types::device::DeviceType;

/// Map a BrainFlow board ID to a NeuroHID device type.
///
/// Returns `None` for boards we don't have a specific DeviceType for.
/// These can still be used via `DeviceType::Unknown(name)`.
pub fn board_id_to_device_type(board_id: BoardIds) -> DeviceType {
    match board_id {
        BoardIds::SyntheticBoard => DeviceType::Mock,
        BoardIds::CytonBoard => DeviceType::OpenBCICyton,
        BoardIds::GanglionBoard => DeviceType::OpenBCIGanglion,
        BoardIds::CytonDaisyBoard => DeviceType::Unknown("OpenBCI Cyton+Daisy (16ch)".into()),
        BoardIds::GaleaBoard => DeviceType::Unknown("Galea".into()),
        BoardIds::MuseSBoard => DeviceType::Muse2,
        BoardIds::Muse2Board => DeviceType::Muse2,
        BoardIds::Muse2016Board => DeviceType::Unknown("Muse 2016".into()),
        BoardIds::BrainbitBoard => DeviceType::Unknown("BrainBit".into()),
        BoardIds::UnicornBoard => DeviceType::Unknown("Unicorn".into()),
        BoardIds::NotionOsBoard => DeviceType::Unknown("Neurosity Notion".into()),
        BoardIds::CrownBoard => DeviceType::Unknown("Neurosity Crown".into()),
        BoardIds::AntNeuroEe410Board => DeviceType::Unknown("ANT Neuro EE410".into()),
        BoardIds::AntNeuroEe411Board => DeviceType::Unknown("ANT Neuro EE411".into()),
        BoardIds::AntNeuroEe430Board => DeviceType::Unknown("ANT Neuro EE430".into()),
        BoardIds::AntNeuroEe211Board => DeviceType::Unknown("ANT Neuro EE211".into()),
        BoardIds::AntNeuroEe212Board => DeviceType::Unknown("ANT Neuro EE212".into()),
        BoardIds::AntNeuroEe213Board => DeviceType::Unknown("ANT Neuro EE213".into()),
        BoardIds::AntNeuroEe214Board => DeviceType::Unknown("ANT Neuro EE214".into()),
        BoardIds::AntNeuroEe215Board => DeviceType::Unknown("ANT Neuro EE215".into()),
        BoardIds::AntNeuroEe221Board => DeviceType::Unknown("ANT Neuro EE221".into()),
        BoardIds::AntNeuroEe222Board => DeviceType::Unknown("ANT Neuro EE222".into()),
        BoardIds::AntNeuroEe223Board => DeviceType::Unknown("ANT Neuro EE223".into()),
        BoardIds::AntNeuroEe224Board => DeviceType::Unknown("ANT Neuro EE224".into()),
        BoardIds::AntNeuroEe225Board => DeviceType::Unknown("ANT Neuro EE225".into()),
        other => DeviceType::Unknown(format!("BrainFlow board {:?}", other)),
    }
}

/// Map a NeuroHID device type to the preferred BrainFlow board ID.
///
/// Returns `None` for device types that aren't backed by BrainFlow
/// (e.g., Emotiv devices, which use the Cortex API instead).
pub fn device_type_to_board_id(device_type: &DeviceType) -> Option<BoardIds> {
    match device_type {
        DeviceType::Mock => Some(BoardIds::SyntheticBoard),
        DeviceType::OpenBCICyton => Some(BoardIds::CytonBoard),
        DeviceType::OpenBCIGanglion => Some(BoardIds::GanglionBoard),
        DeviceType::Muse2 => Some(BoardIds::Muse2Board),
        // Emotiv devices are NOT supported by BrainFlow
        DeviceType::EmotivInsight | DeviceType::EmotivEpocPlus | DeviceType::EmotivEpocX => None,
        DeviceType::Unknown(name) => match name.as_str() {
            "OpenBCI Cyton+Daisy (16ch)" => Some(BoardIds::CytonDaisyBoard),
            "Neurosity Notion" => Some(BoardIds::NotionOsBoard),
            "Neurosity Crown" => Some(BoardIds::CrownBoard),
            "Unicorn" => Some(BoardIds::UnicornBoard),
            "BrainBit" => Some(BoardIds::BrainbitBoard),
            _ => None,
        },
    }
}

/// Get a human-readable device name for a BrainFlow board.
pub fn board_display_name(board_id: BoardIds) -> String {
    match board_id {
        BoardIds::SyntheticBoard => "BrainFlow Synthetic Board".into(),
        BoardIds::CytonBoard => "OpenBCI Cyton (8ch)".into(),
        BoardIds::GanglionBoard => "OpenBCI Ganglion (4ch)".into(),
        BoardIds::CytonDaisyBoard => "OpenBCI Cyton+Daisy (16ch)".into(),
        BoardIds::Muse2Board => "Muse 2".into(),
        BoardIds::MuseSBoard => "Muse S".into(),
        BoardIds::BrainbitBoard => "BrainBit".into(),
        BoardIds::UnicornBoard => "Unicorn".into(),
        BoardIds::NotionOsBoard => "Neurosity Notion".into(),
        BoardIds::CrownBoard => "Neurosity Crown".into(),
        other => format!("{:?}", other),
    }
}

/// List all BrainFlow board IDs that are well-tested with NeuroHID.
///
/// This is used by the provider to limit discovery to boards we've
/// actually validated, rather than exposing all ~30 BrainFlow boards.
pub fn supported_board_ids() -> Vec<BoardIds> {
    vec![
        BoardIds::SyntheticBoard,
        BoardIds::CytonBoard,
        BoardIds::GanglionBoard,
        BoardIds::CytonDaisyBoard,
        BoardIds::Muse2Board,
    ]
}

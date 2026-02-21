//! ONNX inference and action mapping.

use neurohid_types::action::{Action, MouseAction, MouseButton, MouseButtonEvent, MouseMovement};
use neurohid_types::error::Result;
use neurohid_types::signal::FeatureVector;

use super::model::LoadedModel;

pub(super) fn run_inference(model: &LoadedModel, feature: &FeatureVector) -> Result<Action> {
    if feature.dim() != model.manifest.input_dim {
        return Err(neurohid_types::error::Error::Decoder(
            neurohid_types::error::DecoderError::InvalidInputDimensions {
                expected: model.manifest.input_dim,
                got: feature.dim(),
            },
        ));
    }

    let normalized: Vec<f32> = feature
        .values
        .iter()
        .zip(
            model
                .manifest
                .normalization_stats
                .mean
                .iter()
                .zip(model.manifest.normalization_stats.std.iter()),
        )
        .map(|(value, (mean, std))| ((*value - *mean) / *std).clamp(-10.0, 10.0))
        .collect();

    let values = model.model.infer(&normalized)?;
    Ok(action_from_output(&values, feature.timestamp))
}

pub(super) fn lightweight_fallback_action(feature: &FeatureVector) -> Action {
    let dx = feature
        .values
        .first()
        .copied()
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let dy = feature
        .values
        .get(1)
        .copied()
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let confidence = feature
        .values
        .get(2)
        .copied()
        .map(to_probability)
        .unwrap_or_else(|| (dx.abs() + dy.abs()).clamp(0.0, 1.0));
    let mut action = Action::none().with_confidence(confidence);
    if dx.abs() > 0.01 || dy.abs() > 0.01 {
        action.mouse = Some(MouseAction::move_relative(dx, dy));
        action.timestamp = feature.timestamp;
    }
    action
}

pub(super) fn action_from_output(values: &[f32], timestamp: i64) -> Action {
    let dx = *values.first().unwrap_or(&0.0);
    let dy = *values.get(1).unwrap_or(&0.0);

    let (left_click_prob, right_click_prob, confidence_raw) = match values.len() {
        0..=2 => (None, None, None),
        3 => (None, None, values.get(2).copied()),
        4 => (values.get(2).copied(), None, values.get(3).copied()),
        _ => (
            values.get(2).copied(),
            values.get(3).copied(),
            values.get(4).copied(),
        ),
    };

    let confidence = confidence_raw
        .map(to_probability)
        .unwrap_or_else(|| (dx.abs() + dy.abs()).clamp(0.0, 1.0));

    let mut mouse = MouseAction {
        movement: None,
        buttons: Vec::new(),
        scroll: None,
    };

    if dx.abs() > 0.01 || dy.abs() > 0.01 {
        mouse.movement = Some(MouseMovement { dx, dy });
    }

    if left_click_prob.is_some_and(|p| to_probability(p) >= 0.8) {
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Left,
            pressed: true,
        });
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Left,
            pressed: false,
        });
    }

    if right_click_prob.is_some_and(|p| to_probability(p) >= 0.8) {
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Right,
            pressed: true,
        });
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Right,
            pressed: false,
        });
    }

    let mouse = if mouse.movement.is_some() || !mouse.buttons.is_empty() {
        Some(mouse)
    } else {
        None
    };

    Action {
        timestamp,
        mouse,
        keyboard: None,
        confidence,
        decision_id: None,
    }
}

pub(super) fn to_probability(value: f32) -> f32 {
    if (0.0..=1.0).contains(&value) {
        value
    } else {
        1.0 / (1.0 + (-value).exp())
    }
}

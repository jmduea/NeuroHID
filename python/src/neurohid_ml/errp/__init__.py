"""
Error-Related Potential (ErrP) Detection Module

This module is responsible for detecting Error-Related Potentials in the EEG
signal. ErrPs are brain signals that occur when a person perceives that an
action was incorrect. By detecting these signals, we can provide implicit
feedback to the decoder without requiring the user to explicitly say "that
was wrong."

What is an ErrP?
---------------
When you observe an error (whether you made it or the system made it), your
brain generates a characteristic pattern of electrical activity, primarily
from the anterior cingulate cortex (ACC). This pattern includes:

1. The Error-Related Negativity (ERN): A negative voltage deflection that
   peaks around 50-100ms after error awareness, strongest at frontocentral
   electrode sites (Fz, FCz, Cz).

2. The Error Positivity (Pe): A positive deflection following the ERN,
   peaking around 200-400ms, reflecting conscious awareness of the error.

With consumer EEG devices like the Emotiv Insight, we don't have ideal
electrode placement for ErrP detection (we're missing the critical FCz
electrode). However, research has shown that with careful feature extraction
and machine learning, we can still achieve usable detection accuracy (70-80%)
using the available frontal and parietal electrodes.

How We Use ErrPs:
----------------
1. After each action the decoder takes, we wait ~200-600ms for the ErrP window
2. We extract features from the signal in that window
3. We classify whether an ErrP occurred (error detected) or not
4. This classification becomes the reward signal for the RL decoder:
   - ErrP detected → negative reward (the user thought that was wrong)
   - No ErrP → small positive or zero reward (action was acceptable)
"""

from dataclasses import dataclass
from typing import List, Tuple

import numpy as np
from scipy import signal  # type: ignore[import-untyped]
from sklearn.linear_model import LogisticRegression  # type: ignore[import-untyped]
from sklearn.preprocessing import StandardScaler  # type: ignore[import-untyped]


@dataclass
class ErrPConfig:
    """Configuration for ErrP detection.

    These parameters control the feature extraction and classification
    pipeline. The defaults are tuned based on ErrP literature and our
    specific device constraints.
    """

    # Window timing (relative to action timestamp, in milliseconds)
    window_start_ms: int = 150  # Start looking 150ms after action
    window_end_ms: int = 600  # Stop at 600ms

    # Sampling parameters (should match your device)
    sampling_rate_hz: float = 128.0
    num_channels: int = 5  # Emotiv Insight: AF3, AF4, T7, T8, Pz

    # Feature extraction
    use_time_domain: bool = True  # Peak amplitudes, latencies
    use_spectral: bool = True  # Theta power (strongly associated with errors)
    use_cross_channel: bool = True  # AF3-AF4 asymmetry, coherence

    # Classification
    confidence_threshold: float = 0.6  # Minimum confidence to report detection

    # Frequency bands for spectral features
    theta_band: Tuple[float, float] = (4.0, 8.0)  # Theta: error-associated
    delta_band: Tuple[float, float] = (0.5, 4.0)  # Delta
    alpha_band: Tuple[float, float] = (8.0, 13.0)  # Alpha


class ErrPDetector:
    """Detects Error-Related Potentials in EEG signals.

    This class provides the core ErrP detection functionality:
    1. Feature extraction from signal windows
    2. Classification (error vs. no error)
    3. Calibration from labeled examples

    Example usage:
        # During calibration
        detector = ErrPDetector(config)
        for trial in calibration_trials:
            detector.add_calibration_example(
                signal_window=trial.eeg_data,
                was_error=trial.is_error_trial
            )
        detector.fit()

        # During use
        result = detector.detect(signal_window)
        if result.error_probability > 0.5:
            print("User perceived an error!")
    """

    def __init__(self, config: ErrPConfig):
        self.config = config

        # Feature scaler (standardizes features for better classification)
        self.scaler = StandardScaler()

        # Classifier (logistic regression gives us probabilities)
        self.classifier = LogisticRegression(
            class_weight="balanced",  # Handle imbalanced error/correct ratio
            max_iter=1000,
        )

        # Calibration data storage
        self.calibration_features: List[np.ndarray] = []
        self.calibration_labels: List[int] = []  # 1 = error, 0 = no error

        # State
        self.is_calibrated = False

    def extract_features(self, window: np.ndarray) -> np.ndarray:
        """Extract features from a signal window for ErrP detection.

        This is where we transform the raw multi-channel signal into a feature
        vector that captures the characteristics of ErrPs. We combine several
        types of features to be robust to the limited electrode coverage of
        consumer devices.

        Args:
            window: EEG data, shape [num_samples, num_channels]

        Returns:
            Feature vector (1D numpy array)
        """
        features = []
        num_samples, num_channels = window.shape

        # Time-domain features capture the shape of the ERP waveform
        if self.config.use_time_domain:
            features.extend(self._extract_time_features(window))

        # Spectral features capture frequency content (theta power is key)
        if self.config.use_spectral:
            features.extend(self._extract_spectral_features(window))

        # Cross-channel features capture spatial patterns
        if self.config.use_cross_channel:
            features.extend(self._extract_cross_channel_features(window))

        return np.array(features)

    def _extract_time_features(self, window: np.ndarray) -> List[float]:
        """Extract time-domain features from the signal.

        We look for the characteristic ERN (negative peak) and Pe (positive peak)
        components of the ErrP waveform.
        """
        features = []
        num_samples, num_channels = window.shape

        # Define time regions (in samples) for ERN and Pe
        # ERN: roughly 50-150ms, Pe: roughly 200-400ms
        samples_per_ms = self.config.sampling_rate_hz / 1000.0

        ern_start = int(0 * samples_per_ms)  # Relative to window start
        ern_end = int(150 * samples_per_ms)
        pe_start = int(150 * samples_per_ms)
        pe_end = int(400 * samples_per_ms)

        # Clamp to valid indices
        ern_end = min(ern_end, num_samples)
        pe_end = min(pe_end, num_samples)

        for ch in range(num_channels):
            channel_data = window[:, ch]

            # ERN region features (expecting negative peak)
            if ern_end > ern_start:
                ern_region = channel_data[ern_start:ern_end]
                features.append(np.min(ern_region))  # Minimum (ERN peak)
                features.append(np.mean(ern_region))  # Mean amplitude
                features.append(
                    np.argmin(ern_region) / len(ern_region)
                )  # Peak latency (normalized)
            else:
                features.extend([0.0, 0.0, 0.5])

            # Pe region features (expecting positive peak)
            if pe_end > pe_start:
                pe_region = channel_data[pe_start:pe_end]
                features.append(np.max(pe_region))  # Maximum (Pe peak)
                features.append(np.mean(pe_region))  # Mean amplitude
                features.append(np.argmax(pe_region) / len(pe_region))  # Peak latency
            else:
                features.extend([0.0, 0.0, 0.5])

            # Full window statistics
            features.append(np.std(channel_data))  # Variance (activity level)
            features.append(np.max(channel_data) - np.min(channel_data))  # Range

        return features

    def _extract_spectral_features(self, window: np.ndarray) -> List[float]:
        """Extract frequency-domain features.

        Theta band power (4-8 Hz) is strongly associated with error processing.
        We also include delta and alpha for context.
        """
        features = []
        num_samples, num_channels = window.shape

        for ch in range(num_channels):
            channel_data = window[:, ch]

            # Compute power spectral density
            freqs, psd = signal.welch(
                channel_data,
                fs=self.config.sampling_rate_hz,
                nperseg=min(
                    num_samples, 64
                ),  # Shorter window for better time resolution
            )

            # Extract band powers
            for band_name, (low, high) in [
                ("delta", self.config.delta_band),
                ("theta", self.config.theta_band),
                ("alpha", self.config.alpha_band),
            ]:
                band_mask = (freqs >= low) & (freqs <= high)
                if band_mask.any():
                    band_power = np.mean(psd[band_mask])
                else:
                    band_power = 0.0
                features.append(band_power)

            # Theta/alpha ratio (often elevated during errors)
            theta_mask = (freqs >= self.config.theta_band[0]) & (
                freqs <= self.config.theta_band[1]
            )
            alpha_mask = (freqs >= self.config.alpha_band[0]) & (
                freqs <= self.config.alpha_band[1]
            )

            theta_power = np.mean(psd[theta_mask]) if theta_mask.any() else 0.0
            alpha_power = np.mean(psd[alpha_mask]) if alpha_mask.any() else 1e-10

            features.append(theta_power / (alpha_power + 1e-10))

        return features

    def _extract_cross_channel_features(self, window: np.ndarray) -> List[float]:
        """Extract features that capture relationships between channels.

        ErrPs often show characteristic spatial patterns, such as frontal
        asymmetry. Even with limited channels, we can capture some of this.
        """
        features = []
        num_samples, num_channels = window.shape

        # For Emotiv Insight: AF3=0, AF4=1, T7=2, T8=3, Pz=4
        # Frontal asymmetry (AF3 - AF4) is meaningful for error processing
        if num_channels >= 2:
            # Asymmetry in means
            asymmetry = np.mean(window[:, 0]) - np.mean(window[:, 1])
            features.append(asymmetry)

            # Asymmetry in variability
            var_asymmetry = np.std(window[:, 0]) - np.std(window[:, 1])
            features.append(var_asymmetry)
        else:
            features.extend([0.0, 0.0])

        # Correlation between frontal and parietal (AF3/AF4 with Pz)
        # ErrPs show characteristic frontal-parietal connectivity patterns
        if num_channels >= 5:
            frontal_mean = (window[:, 0] + window[:, 1]) / 2
            parietal = window[:, 4]

            if len(frontal_mean) > 1:
                correlation = np.corrcoef(frontal_mean, parietal)[0, 1]
                features.append(correlation if not np.isnan(correlation) else 0.0)
            else:
                features.append(0.0)
        else:
            features.append(0.0)

        return features

    def add_calibration_example(self, signal_window: np.ndarray, was_error: bool):
        """Add a labeled example for calibration.

        During calibration, we collect examples of both error trials (where
        the system deliberately made a mistake) and correct trials. This gives
        us training data for the classifier.

        Args:
            signal_window: EEG data in the ErrP window, shape [samples, channels]
            was_error: True if this trial contained an intentional error
        """
        features = self.extract_features(signal_window)
        self.calibration_features.append(features)
        self.calibration_labels.append(1 if was_error else 0)

    def fit(self) -> dict:
        """Train the classifier on calibration data.

        This should be called after collecting calibration examples. It fits
        the feature scaler and classifier, then returns quality metrics.

        Returns:
            Dictionary with calibration quality metrics
        """
        if len(self.calibration_features) < 10:
            raise ValueError("Need at least 10 calibration examples")

        x = np.array(self.calibration_features)
        y = np.array(self.calibration_labels)

        # Fit the scaler
        x_scaled = self.scaler.fit_transform(x)

        # Fit the classifier
        self.classifier.fit(x_scaled, y)

        # Compute quality metrics (on training data - ideally use cross-validation)
        predictions = self.classifier.predict(x_scaled)

        # Calculate metrics
        accuracy = np.mean(predictions == y)

        # Sensitivity (true positive rate) - how well we detect errors
        error_mask = y == 1
        if error_mask.sum() > 0:
            sensitivity = np.mean(predictions[error_mask] == 1)
        else:
            sensitivity = 0.0

        # Specificity (true negative rate) - how well we avoid false positives
        correct_mask = y == 0
        if correct_mask.sum() > 0:
            specificity = np.mean(predictions[correct_mask] == 0)
        else:
            specificity = 0.0

        self.is_calibrated = True

        return {
            "accuracy": accuracy,
            "sensitivity": sensitivity,
            "specificity": specificity,
            "num_examples": len(y),
            "num_errors": int(error_mask.sum()),
            "num_correct": int(correct_mask.sum()),
        }

    def detect(self, signal_window: np.ndarray) -> "DetectionResult":
        """Detect whether an ErrP occurred in the signal window.

        Args:
            signal_window: EEG data in the ErrP window, shape [samples, channels]

        Returns:
            DetectionResult with error probability and confidence
        """
        if not self.is_calibrated:
            raise RuntimeError("Detector not calibrated. Call fit() first.")

        # Extract and scale features
        features = self.extract_features(signal_window)
        features_scaled = self.scaler.transform(features.reshape(1, -1))

        # Get probability from classifier
        error_probability = self.classifier.predict_proba(features_scaled)[0, 1]

        # Confidence is higher when we're further from the decision boundary (0.5)
        # and the signal quality is good
        confidence = abs(error_probability - 0.5) * 2  # Scale to 0-1

        return DetectionResult(
            error_probability=error_probability,
            confidence=confidence,
            is_reliable=confidence >= self.config.confidence_threshold,
        )

    def save(self, path: str):
        """Save the calibrated detector to a file."""
        import joblib

        joblib.dump(
            {
                "config": self.config,
                "scaler": self.scaler,
                "classifier": self.classifier,
                "is_calibrated": self.is_calibrated,
            },
            path,
        )

    @classmethod
    def load(cls, path: str) -> "ErrPDetector":
        """Load a calibrated detector from a file."""
        import joblib

        data = joblib.load(path)

        detector = cls(data["config"])
        detector.scaler = data["scaler"]
        detector.classifier = data["classifier"]
        detector.is_calibrated = data["is_calibrated"]
        return detector


@dataclass
class DetectionResult:
    """Result of ErrP detection for a single window."""

    error_probability: float  # 0.0 = definitely no error, 1.0 = definitely error
    confidence: float  # How sure we are about this detection
    is_reliable: bool  # Whether this detection meets the confidence threshold

    def to_reward(self) -> float:
        """Convert the detection result to a reward signal for RL.

        We use the error probability directly as a negative reward.
        Higher error probability = more negative reward.
        """
        if not self.is_reliable:
            # If we're not confident, return 0 (no information)
            return 0.0

        # Negative reward proportional to error probability
        return -self.error_probability

# NeuroHID: Product and Technical Specification

**Version:** 0.1.1-draft  
**Last Updated:** February 2026  
**Status:** Pre-Alpha / Architecture Design Phase

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Product Vision and Goals](#2-product-vision-and-goals)
3. [System Architecture Overview](#3-system-architecture-overview)
4. [Signal Processing Pipeline](#4-signal-processing-pipeline)
5. [Machine Learning Subsystem](#5-machine-learning-subsystem)
6. [Error-Related Potential Detection](#6-error-related-potential-detection)
7. [Platform Abstraction Layer](#7-platform-abstraction-layer)
8. [Inter-Process Communication](#8-inter-process-communication)
9. [Storage and Security](#9-storage-and-security)
10. [Calibration System](#10-calibration-system)
11. [User Experience Design](#11-user-experience-design)
12. [Performance Requirements](#12-performance-requirements)
13. [Testing Strategy](#13-testing-strategy)
14. [Deployment and Distribution](#14-deployment-and-distribution)
15. [Risk Assessment](#15-risk-assessment)
16. [Development Roadmap](#16-development-roadmap)
17. [Glossary](#17-glossary)
18. [References](#18-references)

---

## 1. Executive Summary

### 1.1 What is NeuroHID?

NeuroHID is a software system that transforms consumer electroencephalography (EEG) devices into standard computer input peripherals. Users wear an EEG headset, and the system translates their neural signals into mouse movements, clicks, and keyboard inputs. The computer's operating system and applications receive these as ordinary Human Interface Device (HID) events, meaning NeuroHID works with any software without requiring integration or special support.

### 1.2 The Core Innovation

Traditional brain-computer interfaces (BCIs) require explicit user feedback to learn and improve. After each action, the user must indicate whether it was correct or incorrect, creating an exhausting and unnatural interaction loop. NeuroHID eliminates this requirement by detecting **Error-Related Potentials (ErrPs)**, which are brain signals automatically generated when a person perceives an error. When the system moves the cursor in the wrong direction, the user's brain generates a characteristic electrical pattern within 200-300 milliseconds. NeuroHID detects this pattern and uses it as an implicit reward signal for reinforcement learning, enabling the decoder to improve continuously through normal use.

### 1.3 Target Hardware

The initial target device is the **Emotiv Insight**, a five-channel consumer EEG headset with electrodes at positions AF3, AF4, T7, T8, and Pz. This device represents a reasonable compromise between signal quality and accessibility. The architecture supports future expansion to other devices including OpenBCI (research-grade, customizable), Emotiv EPOC+ (14-channel consumer), and Muse 2 (4-channel meditation headset).

### 1.4 Key Technical Decisions

The system uses a **hybrid Rust/Python architecture**. Rust handles all latency-critical operations including signal acquisition, processing, and HID event emission. Python handles machine learning workloads including the reinforcement learning decoder and ErrP classifier. The two processes communicate via local sockets, providing process isolation (Python crashes don't stop the input service) while enabling full access to the PyTorch ecosystem.

---

## 2. Product Vision and Goals

### 2.1 Vision Statement

NeuroHID aims to make brain-computer interaction as natural and accessible as using a mouse or keyboard. Users should be able to put on a headset, complete a brief calibration session, and immediately begin controlling their computer with their thoughts. The system should improve over time, learning the user's unique brain patterns and adapting to changes in signal characteristics.

### 2.2 Primary Goals

Goal 1: Functional Control Within 30 Minutes

A new user should be able to achieve basic cursor control (moving in four directions, clicking) within 30 minutes of first putting on the headset. This includes device setup, signal quality verification, and initial calibration. The "30-minute barrier" is a critical adoption threshold; systems requiring longer setup periods see dramatically lower user retention.

Goal 2: Continuous Improvement Without Explicit Feedback

The system must improve its decoding accuracy through normal use without requiring the user to explicitly label actions as correct or incorrect. This is the core ErrP-based learning innovation. Users should notice improved accuracy after hours to days of use compared to immediately after calibration.

Goal 3: Transparent Integration

Applications should not know that NeuroHID exists. They receive standard HID events indistinguishable from physical input devices. This means NeuroHID works with every application, game, and operating system feature without requiring developer adoption or special accessibility modes.

Goal 4: Privacy by Design

Brain signals are sensitive biometric data. All processing happens locally on the user's computer. No data is transmitted to external servers. Stored data (calibration profiles, model weights) is encrypted at rest using platform-native secure storage for encryption keys.

### 2.3 Non-Goals for MVP

The following capabilities are explicitly out of scope for the minimum viable product:

**Multi-user switching**: The MVP supports a single active profile. Household or institutional sharing with fast profile switching will be addressed in future versions.

**Mobile platforms**: The MVP targets desktop operating systems (Linux, Windows, macOS). Android and iOS support requires different HID emission approaches and is deferred.

**Arbitrary text input**: The MVP supports arrow keys and limited function keys. Full keyboard input with character selection is a significantly harder decoding problem requiring more EEG channels and/or additional modalities.

**Voice integration**: Combining neural signals with voice commands could improve accuracy and expand capabilities, but adds significant complexity and is deferred.

**Observation Space Extensions**: The MVP includes some additional context for the observation space directly related to the targeted use case (e.g. cursor position). A more complete observation space likely could prove beneficial for decoding accuracy but requires more complex integration with the operating system and is deferred.

### 2.4 Success Metrics

The following metrics define MVP success:

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Calibration completion rate | >80% | Users who finish calibration / users who start |
| Time to functional control | <30 min | Calibration start to first successful navigation task |
| Decoding accuracy (post-calibration) | >70% | Correct actions / total actions in controlled test |
| ErrP detection accuracy | >75% | AUC-ROC on held-out calibration data |
| Action latency (95th percentile) | <100ms | Feature ready → HID event emitted |
| System stability | <1 crash/day | Crashes requiring service restart |

---

## 3. System Architecture Overview

### 3.1 High-Level Architecture

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              USER'S COMPUTER                               │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  ┌─────────────┐                                                           │
│  │ EEG Headset │ ──Bluetooth/USB──┐                                        │
│  │  (Emotiv)   │                  │                                        │
│  └─────────────┘                  ▼                                        │
│                         ┌──────────────────┐                               │
│                         │ Vendor Service   │  (Emotiv Cortex API)          │
│                         │ (Data Provider)  │                               │
│                         └────────┬─────────┘                               │
│                                  │ WebSocket (localhost)                   │
│                                  ▼                                         │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      NEUROHID RUST SERVICE                           │  │
│  │  ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌─────────────┐  │  │
│  │  │  Device    │──▶│  Signal    │──▶│    IPC     │──▶│   Action   │ │  │
│  │  │  Adapter   │   │ Processing │   │   Bridge   │   │  Executor   │  │  │
│  │  └────────────┘   └────────────┘   └─────┬──────┘   └──────┬──────┘  │  │
│  │                                          │                  │        │  │
│  │  ┌────────────┐   ┌────────────┐         │                  │        │  │
│  │  │  Storage   │   │   Config   │         │                  │        │  │
│  │  │  Manager   │   │   Loader   │         │                  ▼        │  │
│  │  └────────────┘   └────────────┘         │         ┌─────────────┐   │  │
│  └──────────────────────────────────────────┼─────────│  Platform   │───┤  |
│                                             │         │   Layer     │      │
│                         Unix Socket         │         └─────────────┘      │
│                         / Named Pipe        │                │             │
│                                             ▼                ▼             │
│  ┌──────────────────────────────────────────────────┐   ┌──────────┐       │
│  │              NEUROHID PYTHON PROCESS             │   │    OS    │       │
│  │  ┌────────────┐   ┌────────────┐                 │   │  Input   │       │
│  │  │  Decoder   │◀──│    IPC     │                 │   │  Queue   │      │
│  │  │   (PPO)    │   │   Client   │                 │   └────┬─────┘       │
│  │  └────────────┘   └────────────┘                 │        │             │
│  │  ┌────────────┐                                  │        │             │
│  │  │    ErrP    │                                  │        │             │
│  │  │  Detector  │                                  │        │             │
│  │  └────────────┘                                  │        │             │
│  └──────────────────────────────────────────────────┘        │             │
│                                                              ▼             │
│                                                    ┌─────────────────┐     │
│                                                    │  Applications   │     │
│                                                    │ (Any software)  │     │
│                                                    └─────────────────┘     │
└────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Responsibilities

**Rust Service Components:**

| Component | Crate | Responsibility |
|-----------|-------|----------------|
| Device Adapter | `neurohid-device` | Connects to EEG hardware via vendor APIs, streams raw samples |
| Signal Processing | `neurohid-signal` | Filters, buffers, and extracts features from raw EEG |
| IPC Bridge | `neurohid-ipc` | Communicates with Python process, manages connection lifecycle |
| Action Executor | `neurohid-core` | Applies confidence thresholds, smoothing, and debouncing to decoded actions |
| Platform Layer | `neurohid-platform` | Emits HID events using OS-specific APIs |
| Storage Manager | `neurohid-storage` | Encrypts/decrypts profile data, interfaces with platform keychain |
| Config Loader | `neurohid-types` | Loads and validates configuration from TOML files |

**Python Process Components:**

| Component | Module | Responsibility |
|-----------|--------|----------------|
| IPC Client | `neurohid_ml.bridge` | Receives features from Rust, sends actions and ErrP results back |
| Decoder | `neurohid_ml.decoder` | Neural network policy that maps features to action distributions |
| ErrP Detector | `neurohid_ml.errp` | Classifies whether error-related potentials are present in signal windows |

### 3.3 Data Flow

The following describes the path of data through the system during normal operation:

1. **Sample Acquisition** (8ms period for 128Hz): The EEG headset streams samples to the vendor's service. NeuroHID connects to this service via WebSocket and receives multi-channel voltage readings.

2. **Buffering** (continuous): Samples accumulate in a ring buffer. The buffer maintains the most recent N samples (configurable, typically 1-2 seconds of data).

3. **Feature Extraction** (20-60Hz): Every 15-50ms, the signal processing pipeline extracts features from the most recent window of samples. Features include band powers (delta, theta, alpha, beta, gamma), statistical measures, and cross-channel correlations.

4. **Feature Transmission** (matches extraction rate): Feature vectors are serialized to JSON and sent to the Python process via the IPC socket.

5. **Decoding** (matches feature rate): The Python decoder runs the feature vector through the policy network and samples an action from the output distribution.

6. **Action Transmission** (matches decoding rate): The decoded action is serialized and sent back to the Rust service.

7. **Action Execution** (immediately upon receipt): The Rust service applies confidence thresholds and smoothing, then emits the appropriate HID events via the platform layer.

8. **ErrP Window Capture** (after each action): 150-600ms after each action, the Rust service captures the signal window and sends it to Python for ErrP detection.

9. **ErrP Classification** (async): The Python ErrP detector extracts features from the window and classifies whether an error-related potential is present.

10. **Reward Signal** (async): The ErrP classification result is converted to a reward signal and used to update the decoder via online learning.

### 3.4 Why Two Processes?

The decision to split the system across two processes (Rust and Python) rather than using a single language deserves detailed explanation, as it has significant architectural implications.

**Arguments for the hybrid approach:**

The machine learning ecosystem is overwhelmingly Python-centric. PyTorch, TensorFlow, JAX, scikit-learn, and virtually all modern ML tooling assume Python. Using Rust for ML would require either building custom implementations of standard algorithms or using immature Rust ML libraries with limited documentation and community support. When researchers publish new ErrP detection techniques or RL algorithms, the reference implementations are in Python. The hybrid approach lets us incorporate these advances with minimal translation effort.

Python's garbage collector and global interpreter lock (GIL) introduce latency variability that is unacceptable for the user-facing input path. While median Python performance might be acceptable, occasional GC pauses of 50-100ms would cause noticeable cursor stutter. Rust provides consistent, low-latency performance for the real-time portions of the system.

Process isolation provides fault tolerance. If Python crashes (out of memory, segmentation fault in a native library, unhandled exception), the Rust service continues running. The user's input doesn't suddenly stop. The Rust service can attempt to reconnect to a restarted Python process and resume operation.

The IPC boundary forces clean interface design. The explicit serialization of messages between components creates a natural documentation point and makes it easy to test components in isolation. We can replay recorded feature streams through Python without running the Rust service, and we can feed synthetic actions into the Rust service without running Python.

**Arguments against (and why we proceeded anyway):**

IPC adds latency overhead. Each message requires serialization, a context switch, deserialization, and the reverse path for the response. In practice, this overhead is approximately 0.1-0.5ms for our message sizes over Unix domain sockets. This is negligible compared to the ~5-15ms for neural network inference and the ~50-100ms human perception threshold.

Two-process deployment is more complex than a single binary. Users must ensure both processes are running and can communicate. We mitigate this through the Rust service managing the Python process lifecycle (spawning it on startup, monitoring health, restarting on crash).

Debugging cross-process issues is harder than debugging a single process. We mitigate this through extensive logging at the IPC boundary and tools to replay message traces.

---

## 4. Signal Processing Pipeline

### 4.1 Overview

The signal processing pipeline transforms raw voltage readings from the EEG device into feature vectors suitable for machine learning. This pipeline runs entirely in Rust for consistent, low-latency performance.

### 4.2 Input Characteristics

For the Emotiv Insight, input samples have the following characteristics:

| Property | Value |
|----------|-------|
| Channels | 5 (AF3, AF4, T7, T8, Pz) |
| Sampling Rate | 128 Hz |
| Resolution | 14 bits |
| Voltage Range | ±4.17 mV |
| Noise Floor | ~1-2 µV RMS |

Samples arrive via WebSocket in JSON format from the Emotiv Cortex API. Each sample includes channel voltages, quality indicators per channel, and timestamps.

When data enters through LSL multi-stream publishers, NeuroHID performs
metadata-based stream classification before decoding-oriented feature
extraction. EEG-like streams are routed to the spectral/statistical decoder
feature path. Auxiliary streams (for example motion, quality, metrics, and
command channels) remain connected and observable for downstream tooling and
future functionality, but are not forced through EEG-only feature assumptions.

### 4.3 Preprocessing

**Artifact Rejection**: Consumer EEG is contaminated by various artifacts including eye blinks (large amplitude deflections in frontal channels), muscle activity (high-frequency broadband noise), and electrode movement (baseline drift). We implement simple artifact detection by flagging samples where any channel exceeds a threshold (typically ±100 µV) or where channel-to-channel correlation drops below expected levels. Flagged samples are excluded from feature windows.

**Baseline Correction**: Electrode impedance and skin potential drift cause slow baseline wander. We apply high-pass filtering (0.5 Hz cutoff, second-order Butterworth) to remove this drift while preserving relevant neural frequencies.

**Line Noise Removal**: Power line interference (50 or 60 Hz depending on region) appears as a strong narrowband signal. We apply a notch filter centered at the local line frequency with Q factor of 30.

**Bandpass Filtering**: After baseline correction and notch filtering, we apply a bandpass filter (0.5-45 Hz) to isolate frequencies of interest while attenuating residual high-frequency muscle artifact.

All filters are implemented as IIR (Infinite Impulse Response) filters using the bilinear transform from continuous-time prototypes. We use direct form II transposed structure for numerical stability and maintain filter state between sample arrivals to avoid edge effects.

### 4.4 Feature Extraction

Features are extracted from sliding windows of filtered samples. Default window parameters are 500ms length with 50ms step (yielding 20 features/second).

**Band Power Features**: We compute power spectral density (PSD) using Welch's method (Hanning window, 50% overlap) and integrate power in standard frequency bands:

| Band | Frequency Range | Functional Association |
|------|-----------------|------------------------|
| Delta | 0.5-4 Hz | Deep sleep, some cognitive tasks |
| Theta | 4-8 Hz | Memory, error processing, drowsiness |
| Alpha | 8-13 Hz | Relaxation, eyes closed, inhibition |
| Beta | 13-30 Hz | Active thinking, motor planning |
| Gamma | 30-45 Hz | Perception, consciousness, binding |

For each channel and band, we extract total power, relative power (band power / total power), and peak frequency within the band.

**Time-Domain Features**: We compute statistical measures directly from the time-domain signal:

- Mean amplitude (typically near zero after filtering, deviations indicate artifact)
- Standard deviation (indicates overall activity level)
- Skewness and kurtosis (distribution shape, sensitive to artifacts)
- Hjorth parameters: Activity (variance), Mobility (mean frequency), Complexity (bandwidth)
- Zero-crossing rate (correlates with dominant frequency)
- Peak-to-peak amplitude and latency of maximum

**Cross-Channel Features**: Spatial patterns carry significant information about neural sources:

- Channel correlations (5×5 correlation matrix, 10 unique values)
- Frontal asymmetry: log(AF4 power) - log(AF3 power), separately per band
- Coherence between channel pairs in each frequency band

**Temporal Features**: Changes over time relative to baseline:

- Power in current window relative to 10-second trailing average (event-related changes)
- Slope of power over the last 5 windows (trending)

### 4.5 Feature Vector Composition

The complete feature vector concatenates all extracted features. For the Emotiv Insight configuration:

| Feature Category | Count |
|-----------------|-------|
| Band powers (5 bands × 5 channels × 3 measures) | 75 |
| Time-domain (5 channels × 8 measures) | 40 |
| Cross-channel (10 correlations + 5 asymmetries × 5 bands) | 35 |
| Temporal (5 bands × 5 channels × 2 measures) | 50 |
| **Total** | **200** |

Features are z-score normalized using running statistics maintained over a 60-second window, ensuring the decoder sees standardized inputs regardless of absolute amplitude variations.

### 4.6 Latency Budget

The signal processing pipeline must complete within its allocated latency budget to maintain real-time operation:

| Operation | Budget | Typical |
|-----------|--------|---------|
| Sample receive and parse | 1ms | 0.3ms |
| Ring buffer insert | 0.1ms | 0.02ms |
| Filter application (per sample) | 0.5ms | 0.1ms |
| Feature extraction (per window) | 5ms | 2ms |
| Serialization for IPC | 1ms | 0.3ms |
| **Total per feature window** | **~8ms** | **~3ms** |

At 20 Hz feature extraction rate, we have 50ms per window. The typical 3ms processing time provides substantial headroom.

---

## 5. Machine Learning Subsystem

### 5.1 Overview

The machine learning subsystem decodes user intentions from EEG features and translates them into actions. It uses Proximal Policy Optimization (PPO), a policy gradient reinforcement learning algorithm, with the reward signal derived from ErrP detection rather than explicit user feedback.

### 5.2 Problem Formulation

We formulate brain-computer interface control as a Partially Observable Markov Decision Process (POMDP):

**State Space**: The true state includes the user's intended action, their current cognitive state, electrode impedances, and environmental factors. This state is not directly observable.

**Observation Space**: The agent observes feature vectors extracted from EEG signals (approximately 200 dimensions as described in Section 4.5) plus contextual information including current cursor position (normalized), cursor velocity, and screen dimensions. The full observation vector is approximately 210 dimensions.

**Action Space**: The action space is hybrid continuous-discrete:

- Continuous: Mouse movement (dx, dy), each in range [-10, +10] pixels
- Discrete: No action, left click, right click, up arrow, down arrow, left arrow, right arrow

**Reward Signal**: Derived from ErrP detection (detailed in Section 6). Error detection yields negative reward (typically -1), absence of error yields small positive reward (+0.1), and uncertain detection yields zero reward.

**Discount Factor**: γ = 0.99. High discount factor reflects that we care about long-term behavior and occasional errors are acceptable if overall performance is good.

### 5.3 Policy Network Architecture

The policy network is a multi-layer perceptron (MLP) with separate heads for continuous and discrete actions, plus a value head for the critic:

```
Input (210 dim)
    │
    ▼
Linear(210 → 128) + LayerNorm + ReLU
    │
    ▼
Linear(128 → 128) + LayerNorm + ReLU
    │
    ├──────────────────────┬──────────────────────┐
    ▼                      ▼                      ▼
Linear(128 → 2)      Linear(128 → 7)       Linear(128 → 1)
    │                      │                      │
    ▼                      ▼                      ▼
Continuous Mean      Discrete Logits         Value
    │
    ▼
+ Learnable Log-Std (2 dim)
    │
    ▼
Normal(mean, exp(log_std))
```

The continuous action head outputs mean and log-standard-deviation parameters for a diagonal Gaussian distribution over (dx, dy). The discrete action head outputs logits for a categorical distribution over the 7 discrete actions. The value head outputs a scalar estimate of expected return from the current state.

**Parameter Count**: Approximately 50,000 parameters. This is deliberately small to enable fast inference (~5ms on CPU) and to reduce overfitting risk given limited training data.

**Initialization**: Weights initialized using orthogonal initialization with gain 1.0 for hidden layers and 0.01 for output layers. This encourages small initial actions, reducing uncontrolled behavior during early training.

### 5.4 PPO Training Algorithm

PPO is a policy gradient algorithm that constrains policy updates to prevent destructive large steps. We use the clipped surrogate objective variant.

**Objective Function**:

```
L(θ) = E[min(r_t(θ)Â_t, clip(r_t(θ), 1-ε, 1+ε)Â_t)] - c₁L_VF(θ) + c₂S[π_θ]
```

Where:

- r_t(θ) = π_θ(a_t|s_t) / π_θ_old(a_t|s_t) is the probability ratio
- Â_t is the advantage estimate (computed using GAE)
- ε = 0.2 is the clipping parameter
- L_VF is the value function loss (MSE between predicted and actual returns)
- S[π_θ] is the policy entropy (encourages exploration)
- c₁ = 0.5 and c₂ = 0.01 are loss coefficients

**Training Loop**:

1. Collect T timesteps of experience (features, actions, rewards)
2. Compute returns and advantages using GAE(λ=0.95)
3. For each of K epochs (K=4):
   a. Shuffle experiences into minibatches
   b. For each minibatch:
      - Compute policy loss with clipping
      - Compute value loss
      - Compute entropy bonus
      - Backpropagate combined loss
      - Clip gradients to max norm 0.5
      - Update parameters with Adam optimizer
4. Discard old experiences and repeat

**Online Learning Considerations**:

Unlike typical RL settings where training happens offline on collected data, NeuroHID trains online while the user is actively using the system. This introduces several challenges:

- **Non-stationarity**: The user's brain patterns may change due to learning, fatigue, or electrode drift. We address this by maintaining a short experience buffer (last 5 minutes of use) and periodically retraining on this recent data.

- **Sparse, noisy rewards**: ErrP detection is imperfect and rewards arrive asynchronously. We aggregate rewards over time windows and use the confidence of ErrP detection to weight the reward signal.

- **Exploration-exploitation tradeoff**: Too much exploration makes control frustrating; too little prevents learning. We use entropy regularization and anneal the entropy coefficient over time.

### 5.5 Action Space Details

**Continuous Actions (Mouse Movement)**:

The network outputs mean and standard deviation for a Gaussian distribution over (dx, dy). During inference, we either sample from this distribution (exploration) or use the mean directly (exploitation). The raw outputs are scaled by a sensitivity parameter (default 2.0) before being emitted as mouse movement.

Movement smoothing is applied in the Rust service using exponential moving average: smoothed = α × new + (1-α) × smoothed_prev, with α = 0.7 by default.

**Discrete Actions**:

The discrete action head outputs logits for 7 actions:

| Index | Action | Debounce (ms) |
|-------|--------|---------------|
| 0 | No action | 0 |
| 1 | Left click | 200 |
| 2 | Right click | 200 |
| 3 | Up arrow | 100 |
| 4 | Down arrow | 100 |
| 5 | Left arrow | 100 |
| 6 | Right arrow | 100 |

Discrete actions are only emitted when the confidence (probability of the selected action) exceeds a threshold (default 0.6) and sufficient time has passed since the last action of the same type (debounce interval).

### 5.6 Confidence Estimation

The decoder provides confidence estimates with each action, which the Rust service uses to decide whether to emit the action:

**Continuous action confidence**: Inverse of the standard deviation. Narrow distributions indicate high confidence in the intended movement direction.

**Discrete action confidence**: Probability mass on the selected action. If the maximum probability is 0.4 for a 7-way classification, the network is uncertain.

**Combined confidence**: Weighted average of continuous and discrete confidence, with weights based on which component is more relevant to the current action.

---

## 6. Error-Related Potential Detection

### 6.1 Neurophysiological Background

Error-Related Potentials (ErrPs) are event-related potentials (ERPs) generated when a person perceives an error. They were first described in the context of self-generated errors (making a mistake on a task) but also occur when observing errors made by external agents, including computers. This "observation ErrP" is what NeuroHID leverages.

The ErrP consists of two primary components:

**Error-Related Negativity (ERN)**: A negative voltage deflection peaking approximately 50-100ms after error perception, with maximum amplitude at frontocentral electrode sites (Fz, FCz, Cz). The ERN is generated by the anterior cingulate cortex (ACC) and is associated with automatic error detection processes.

**Error Positivity (Pe)**: A positive voltage deflection following the ERN, peaking approximately 200-400ms after error perception, with a more posterior scalp distribution. The Pe is associated with conscious awareness of the error and its significance.

In the context of BCI control, ErrPs occur when the user perceives that the system's action did not match their intention. For example, if the user intends to move the cursor right but the decoder moves it left, an ErrP is generated approximately 200-350ms after the erroneous movement becomes visually apparent.

### 6.2 Challenges with Consumer Hardware

The Emotiv Insight lacks electrodes at the optimal positions for ErrP detection. The FCz electrode, where ErrP amplitude is maximal, is absent. We must rely on nearby frontal electrodes (AF3, AF4) and infer the ErrP signal indirectly.

This constraint reduces detection accuracy compared to research-grade systems. Laboratory studies with optimal electrode placement achieve 85-95% detection accuracy; we target 70-80% with consumer hardware. This is sufficient for our use case because we aggregate error signals over many actions rather than making critical decisions based on single trials.

### 6.3 Detection Pipeline

**Window Capture**: After each action is emitted, we wait for a configurable delay (default 150ms) to allow visual feedback to reach the user's brain, then capture a signal window of configurable length (default 450ms, so the window spans 150-600ms post-action).

**Feature Extraction**: We extract features optimized for ErrP detection from this window:

*Time-domain features*:

- Minimum amplitude in 50-150ms window (ERN peak)
- Maximum amplitude in 200-400ms window (Pe peak)
- Mean amplitude in each 50ms bin across the window
- Peak-to-peak amplitude (Pe - ERN)
- Latency of minimum (ERN timing)
- Latency of maximum (Pe timing)

*Frequency-domain features*:

- Theta band (4-8Hz) power, which increases during error processing
- Theta/alpha ratio
- Delta band power (sometimes elevated during errors)

*Cross-channel features*:

- Frontal asymmetry (AF3 vs AF4) which may differ between error and correct trials
- Correlation between frontal and parietal channels

The total feature vector for ErrP detection is approximately 50-80 dimensions depending on configuration.

**Classification**: We use a logistic regression classifier trained during calibration. Logistic regression provides probability outputs (rather than just binary classification), which we use as confidence measures. The classifier is trained with balanced class weights to handle the typical 70-30 correct-to-error ratio in calibration data.

**Confidence Thresholding**: We only report ErrP detection results when the classifier confidence exceeds a threshold (default 0.6). Low-confidence detections are discarded rather than used as reward signals, preventing noisy gradient updates.

### 6.4 Calibration Protocol

ErrP detection requires user-specific calibration because ErrP morphology varies significantly between individuals. The calibration process uses the Grid Maze game to collect labeled examples:

1. Present the user with a navigation task (move a cursor to a target)
2. On each trial, instruct the user to think about a direction
3. The system moves the cursor in the instructed direction (correct trial) or a different direction (error trial)
4. Record the EEG signal in the post-action window
5. Label the trial as error or correct based on whether the action matched instruction

We collect approximately 50 trials (35 correct, 15 error) during initial calibration. The error rate (30%) is higher than what would occur during normal use to ensure sufficient error examples for classifier training.

After collection, we train the classifier using 5-fold cross-validation and report calibration quality metrics:

- **Accuracy**: Overall correct classification rate
- **Sensitivity (True Positive Rate)**: Proportion of errors correctly detected
- **Specificity (True Negative Rate)**: Proportion of correct trials correctly identified
- **AUC-ROC**: Area under the receiver operating characteristic curve

If calibration quality is poor (AUC < 0.65), we advise the user to recalibrate with improved electrode contact or in a less noisy environment.

### 6.5 Online Adaptation

ErrP characteristics may drift over time due to changes in electrode impedance, user attention level, and learning effects. We implement slow online adaptation:

1. Maintain a buffer of the last N (default 100) ErrP feature vectors and their predicted labels
2. Periodically (every 10 minutes) retrain the classifier on this buffer combined with the original calibration data
3. Use elastic net regularization to prevent overfitting to recent data
4. Reject updates that significantly degrade cross-validation performance

This adaptation is conservative to avoid catastrophic forgetting of the original calibration.

---

## 7. Platform Abstraction Layer

### 7.1 Overview

The platform abstraction layer handles all operating-system-specific functionality, primarily HID event emission and system state queries. This layer isolates platform differences from the rest of the codebase, enabling cross-platform support with minimal conditional compilation.

### 7.2 HID Emission

**Linux**: We use the `uinput` kernel module to create virtual input devices. This approach is robust and works with all desktop environments (X11, Wayland, console). The virtual device appears to applications exactly like a physical mouse or keyboard.

Requirements:

- User must have read/write access to `/dev/uinput`
- Typically requires membership in the `input` group or a udev rule

Implementation notes:

- We create a single virtual device that combines mouse and keyboard capabilities
- Events are written synchronously to `/dev/uinput` file descriptor
- We include `SYN_REPORT` events after each logical input to ensure timely delivery

**Windows**: We use the `SendInput` API from User32.dll. This synthesizes keyboard and mouse input events that are inserted into the system input queue.

Requirements:

- No special permissions for standard applications
- Cannot inject input into applications running with higher integrity level (UAC elevation)
- Does not work on the secure desktop (login screen, UAC prompts)

Implementation notes:

- Mouse movement uses `MOUSEEVENTF_MOVE` flag for relative movement
- Keyboard events use virtual key codes
- We handle DPI awareness to ensure consistent movement across display scaling settings

**macOS**: We use the Quartz Event Services API (`CGEvent`) to post events.

Requirements:

- Application must have Accessibility permission (System Preferences → Security & Privacy → Privacy → Accessibility)
- First run prompts the user to grant permission
- Permission state can be checked with `AXIsProcessTrusted()`

Implementation notes:

- Mouse events created with `CGEventCreateMouseEvent`
- Keyboard events created with `CGEventCreateKeyboardEvent`
- Events posted with `CGEventPost(kCGHIDEventTap, event)`
- Some applications (password fields, secure input mode) block synthetic input

### 7.3 System Queries

The platform layer also provides queries for cursor position and screen information, used to construct the observation context for the decoder:

**Cursor Position**:

- Linux (X11): `XQueryPointer`
- Linux (Wayland): Not directly queryable; we track position based on our emitted movements
- Windows: `GetCursorPos`
- macOS: `CGEventGetLocation` or `NSEvent.mouseLocation`

**Screen Information**:

- Linux (X11): `XRRGetScreenResources` for multi-monitor setup
- Windows: `GetSystemMetrics` and `EnumDisplayMonitors`
- macOS: `NSScreen.screens` and `CGDisplayBounds`

### 7.4 Permission Handling

Each platform has different permission requirements. The platform layer provides methods to check permissions and generate user-friendly instructions when permissions are missing:

```rust
pub trait Platform {
    /// Check if we have permission to emit input events
    fn check_input_permissions(&self) -> Result<(), PermissionError>;
    
    /// Check if we have permission to query system state
    fn check_query_permissions(&self) -> Result<(), PermissionError>;
}
```

The `PermissionError` type includes a `hint` field with platform-specific instructions for resolving the issue:

Linux example:

```
Permission denied accessing /dev/uinput.

To fix this, either:

1. Add your user to the 'input' group:
   sudo usermod -a -G input $USER
   (Log out and back in for this to take effect)

2. Or create a udev rule at /etc/udev/rules.d/99-neurohid.rules:
   KERNEL=="uinput", MODE="0666"
   Then run: sudo udevadm control --reload-rules
```

macOS example:

```
Accessibility permission required.

To grant permission:
1. Open System Preferences
2. Go to Security & Privacy → Privacy → Accessibility
3. Click the lock to make changes
4. Add and enable NeuroHID in the list

The permission prompt may appear automatically on first run.
```

---

## 8. Inter-Process Communication

### 8.1 Protocol Overview

The Rust service and Python process communicate over a local socket using a simple length-prefixed JSON protocol:

```
┌─────────────────────────────────────────┐
│  4 bytes: message length (little-endian) │
├─────────────────────────────────────────┤
│  N bytes: JSON-encoded message body      │
└─────────────────────────────────────────┘
```

We chose JSON for message encoding because it provides human-readable messages for debugging, easy inspection with standard tools, schema flexibility during development, and adequate performance for our message rates. The serialization overhead (approximately 0.1ms for our typical message sizes) is negligible compared to other latencies in the system.

### 8.2 Message Types

**Rust → Python Messages**:

| Type | Purpose | Frequency |
|------|---------|-----------|
| `FeatureBatch` | Features for decoding | 20-60 Hz |
| `ErrPWindow` | Signal window for ErrP detection | After each action |
| `TrainingBatch` | Experience data for training | Periodic (every ~5 min) |
| `ModelUpdate` | Request to load new model weights | After calibration |
| `StatusUpdate` | Service state changes | On state change |
| `Ping` | Health check | Every 10 seconds |
| `Shutdown` | Request clean shutdown | On service stop |

**Python → Rust Messages**:

| Type | Purpose | Frequency |
|------|---------|-----------|
| `Action` | Decoded action to execute | Matches FeatureBatch rate |
| `ErrPResult` | Error detection result | Matches ErrPWindow rate |
| `TrainingComplete` | Training finished notification | After training |
| `ModelLoaded` | Confirmation of model load | After ModelUpdate |
| `Error` | Error occurred in Python | On error |
| `Pong` | Response to health check | Matches Ping |
| `Ready` | Python is ready to receive | On startup |

### 8.3 Message Schemas

**FeatureBatch** (Rust → Python):

```json
{
  "type": "FeatureBatch",
  "features": [
    {"values": [0.1, -0.2, ...], "timestamp": 1706647200000000}
  ],
  "context": {
    "cursor_x": 0.5,
    "cursor_y": 0.3,
    "cursor_velocity_x": 10.2,
    "cursor_velocity_y": -5.1,
    "screen_width": 1920,
    "screen_height": 1080,
    "signal_quality": "Good",
    "timestamp": 1706647200000000
  },
  "sequence": 12345
}
```

**Action** (Python → Rust):

```json
{
  "type": "Action",
  "action": {
    "mouse": {
      "movement": {"dx": 2.5, "dy": -1.0},
      "buttons": [],
      "scroll": null
    },
    "keyboard": null,
    "confidence": 0.85,
    "timestamp": 1706647200000000
  },
  "sequence": 12345,
  "inference_latency_us": 5200
}
```

**ErrPResult** (Python → Rust):

```json
{
  "type": "ErrPResult",
  "result": {
    "error_probability": 0.72,
    "classification_confidence": 0.81,
    "signal_quality": "Good",
    "magnitude": 0.65
  },
  "sequence": 12340
}
```

### 8.4 Connection Lifecycle

1. **Rust service starts**: Creates the IPC socket (Unix domain socket on Linux/macOS, named pipe on Windows), begins listening.

2. **Python process spawned**: Rust service spawns the Python process as a child, passing the socket path as an argument.

3. **Python connects**: Python process opens the socket and sends a `Ready` message.

4. **Normal operation**: Messages flow bidirectionally according to the patterns above.

5. **Health monitoring**: Rust sends `Ping` messages every 10 seconds. If no `Pong` is received within 5 seconds, the connection is considered dead.

6. **Disconnection handling**: If Python disconnects unexpectedly, Rust logs the event and attempts to respawn Python (up to 3 times with exponential backoff).

7. **Shutdown**: On service stop, Rust sends `Shutdown` message and waits up to 5 seconds for Python to close gracefully before terminating the process.

### 8.5 Socket Paths

Default socket locations:

| Platform | Path |
|----------|------|
| Linux | `$XDG_RUNTIME_DIR/neurohid.sock` or `/tmp/neurohid.sock` |
| macOS | `/tmp/neurohid.sock` |
| Windows | `\\.\pipe\neurohid` |

The path is configurable via the configuration file.

---

## 9. Storage and Security

### 9.1 Data Sensitivity

NeuroHID handles several categories of sensitive data:

**Brain signal data**: EEG recordings contain biometric information that could potentially be used for identification or to infer cognitive states, health conditions, or emotional responses. This data must be treated with the same care as fingerprint or facial recognition data.

**Device credentials**: Connection to Emotiv devices requires API credentials (client ID/secret). These must be stored securely to prevent unauthorized device access.

**Trained models**: The decoder and ErrP classifier are trained on the user's specific brain patterns. While less sensitive than raw signal data, they still represent personal biometric information.

### 9.2 Security Architecture

We implement a defense-in-depth approach:

**Encryption at rest**: All sensitive data stored on disk is encrypted using AES-256-GCM. The encryption key is stored in the platform's secure credential storage (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux) rather than on the filesystem.

**Process isolation**: The Rust service runs as a regular user process with minimal privileges. It does not require root/administrator access. Input emission permissions are granted through group membership or user-level permissions rather than elevation.

**No network transmission**: All processing happens locally. The only network connections are to the local vendor service (Emotiv Cortex) running on localhost. The system does not phone home, check for updates, or transmit any telemetry.

**Minimal data retention**: Session logs are encrypted and automatically rotated/deleted after a configurable period (default 7 days). Raw signal data is only retained during active sessions and is not persisted to disk.

### 9.3 Directory Structure

```
~/.config/neurohid/           (Linux: $XDG_CONFIG_HOME/neurohid)
~/Library/Application Support/neurohid/    (macOS)
%APPDATA%\neurohid\           (Windows)
│
├── config.toml               # Plain text - user preferences, non-sensitive
│
├── profiles/
│   └── {profile-id}/
│       ├── metadata.json     # Plain text - profile name, timestamps
│       ├── calibration.enc   # Encrypted - calibration trial data
│       ├── errp_model.enc    # Encrypted - trained ErrP classifier
│       └── decoder_model.enc # Encrypted - trained decoder weights
│
└── logs/
    └── session_*.enc         # Encrypted - session logs (auto-rotate)
```

### 9.4 Encryption Implementation

**Key derivation**: On first run, we generate a random 256-bit master key and store it in the platform keychain under the service name "neurohid". The key is retrieved from the keychain on each startup.

**File encryption**: Each encrypted file uses a random 96-bit nonce prepended to the ciphertext. The format is:

```
[12 bytes: nonce][N bytes: AES-256-GCM ciphertext with 16-byte auth tag]
```

**Key rotation**: Not implemented in MVP. Future versions may support key rotation for long-term security.

### 9.5 Credential Storage

Device API credentials (Emotiv client ID and secret) are stored in the platform keychain as separate entries:

- Service: "neurohid"  
- Account: "emotiv_client_id" / "emotiv_client_secret"

This keeps credentials out of configuration files that might be accidentally shared or backed up to insecure locations.

---

## 10. Calibration System

### 10.1 Purpose and Goals

Calibration serves two purposes: training the ErrP classifier and establishing baseline decoder performance. A well-designed calibration process is critical because it sets the foundation for all subsequent system performance, it is the user's first extended interaction with the system, and calibration quality directly impacts user retention.

The calibration system aims to complete calibration within 15-30 minutes, maintain user engagement through game-like tasks, collect sufficient data for robust classifier training, provide clear feedback about calibration quality, and allow early termination if signal quality is inadequate.

### 10.2 Calibration Flow

The calibration wizard guides users through the following steps:

**Step 1: Device Connection and Signal Check (2-5 minutes)**

The system connects to the EEG device and displays real-time signal quality for each channel. Users receive guidance on headset positioning to achieve good electrode contact. Calibration only proceeds when average signal quality exceeds a threshold (70% of channels reporting "Good" quality for at least 30 seconds).

**Step 2: Grid Maze Game - Discrete ErrP Calibration (8-12 minutes)**

The user navigates a simple grid-based maze by thinking about directions. On each trial the system displays an arrow indicating the direction to think about, waits for 1-2 seconds while the user focuses, moves the cursor (correctly 70% of the time, incorrectly 30%), and records the EEG response. This game collects approximately 50 labeled trials for ErrP classifier training.

**Step 3: Target Tracking Game - Continuous Control Calibration (5-8 minutes)**

The user attempts to keep a cursor on a moving target. Periodically, the system injects perturbations (sudden cursor jumps away from the target) to elicit ErrPs in a continuous control context. This complements the Grid Maze data with continuous control scenarios.

**Step 4: Classifier Training (1-2 minutes)**

The system trains the ErrP classifier on collected data using 5-fold cross-validation. Progress is displayed to the user. Training completes when validation accuracy stabilizes.

**Step 5: Validation and Results (1-2 minutes)**

A brief validation task confirms the system works end-to-end. The user performs 10 simple navigation actions and receives feedback on success rate. Calibration quality metrics are displayed including ErrP detection accuracy, signal quality summary, and estimated decoder performance.

### 10.3 Grid Maze Game Design

The Grid Maze is designed to elicit clear, consistent ErrPs:

**Visual Design**: A 5×5 grid with the player position (blue circle), goal position (green square), and clear boundaries. High contrast colors on a dark background minimize visual fatigue.

**Trial Structure**: Each trial displays an arrow showing the instructed direction for 1.5 seconds, a focus period of 2 seconds where the user maintains concentration on the direction, action execution over 0.5 seconds where the cursor moves (the user observes the result), feedback for 1.5 seconds showing whether the movement was correct, and finally an inter-trial interval of 1 second.

**Error Injection**: 30% of trials are error trials where the cursor moves in a different direction than instructed. Errors are distributed pseudo-randomly to avoid predictable patterns. We avoid consecutive errors to maintain user engagement.

**Timing Rationale**: The 200ms delay between visual feedback and ErrP window capture accounts for visual processing time. The 450ms window captures both ERN (early) and Pe (late) components.

### 10.4 Target Tracking Game Design

The Target Tracking game provides continuous control calibration:

**Visual Design**: A circular target (green) moves smoothly around the screen. A circular cursor (blue) tracks the target. A faint trail shows recent cursor path for spatial reference.

**Target Motion**: The target follows a smooth trajectory with occasional direction changes. Velocity is calibrated to be challenging but achievable. The target bounces off screen edges to keep it visible.

**Perturbation Injection**: Every 2-5 seconds (randomized), the cursor is pushed away from the target. Perturbation strength is calibrated to be clearly perceptible but not frustrating. The ErrP window is captured 150ms after perturbation onset.

**Duration**: 2 minutes of continuous tracking, yielding approximately 24-40 perturbation events.

### 10.5 Quality Metrics

After calibration, we report the following metrics:

| Metric | Good | Acceptable | Poor |
|--------|------|------------|------|
| ErrP AUC-ROC | > 0.80 | 0.70-0.80 | < 0.70 |
| ErrP Sensitivity | > 75% | 60-75% | < 60% |
| ErrP Specificity | > 80% | 65-80% | < 65% |
| Signal Quality | > 80% | 60-80% | < 60% |
| Trial Completion | > 90% | 75-90% | < 75% |

Users with "Poor" calibration quality receive recommendations for improvement: ensuring better electrode contact, calibrating in a quieter environment, checking for sources of electrical interference, and trying a different time of day when more alert.

---

## 11. User Experience Design

### 11.1 Design Principles

**Principle 1: Invisible When Working**

When NeuroHID is functioning well, the user should forget it exists. The cursor moves where intended, clicks happen when expected, and the system fades into the background like a well-functioning mouse. This means minimal UI, no unnecessary notifications, and silent background operation.

**Principle 2: Graceful Degradation**

When signal quality drops or the decoder is uncertain, the system should become less responsive rather than erratic. No action is better than a wrong action. Users should experience the system becoming "sluggish" during poor conditions rather than making random movements.

**Principle 3: Clear Recovery Paths**

When something goes wrong, the path to recovery should be obvious. If the device disconnects, the tray icon shows a clear "Disconnected" state with a "Reconnect" action. If calibration quality degrades, the system suggests recalibration. Error messages include actionable next steps.

**Principle 4: Respect User Attention**

EEG signal quality depends on user cognitive state. The system should not demand attention through notifications, animations, or sounds unless absolutely necessary. Status is available when the user chooses to check it, not pushed at them.

### 11.2 System Tray Application

The primary user interface during normal operation is a system tray icon that communicates system state through icon appearance, using distinct icons for "Active" (normal operation), "Paused" (user-initiated pause), "Learning" (online training in progress), "Disconnected" (device not connected), and "Error" (problem requiring attention).

The context menu provides quick actions including Pause/Resume, Open Settings, View Status, Start Calibration, and Quit. Left-clicking the icon shows a brief status tooltip with signal quality and session statistics.

### 11.3 Settings Interface

Settings are organized into categories:

**Device Settings**: Device selection (when multiple supported devices are present), connection preferences including auto-reconnect and timeout, and signal quality thresholds.

**Decoder Settings**: Mouse sensitivity, smoothing factor, confidence threshold for actions, and online learning enable/disable.

**Action Settings**: Enabled action types (mouse movement, clicks, arrow keys), debounce intervals, and keyboard shortcuts for pause/resume.

**Profile Management**: Create/delete/rename profiles, export/import profiles (with security warning), and set default profile.

**Advanced Settings**: Log level, data retention period, and IPC configuration.

### 11.4 First-Run Experience

The first-run experience guides new users through setup in a sequence of steps:

1. **Welcome Screen**: Brief explanation of what NeuroHID does, time estimate for setup (20-30 minutes), and hardware requirements check.

2. **Device Setup**: Install vendor software if needed (with download link), connect headset, verify device is recognized.

3. **Permission Grant**: Platform-specific permission requests with explanations of why each permission is needed.

4. **Signal Check**: Verify electrode contact, provide positioning guidance, require minimum signal quality before proceeding.

5. **Calibration**: Grid Maze and Target Tracking games as described in Section 10.

6. **Tutorial**: Brief interactive tutorial demonstrating basic control including moving cursor to targets, clicking on buttons, and using arrow keys.

7. **Completion**: Summary of calibration results, tips for best performance, and how to access settings later.

### 11.5 Notification Policy

Notifications are used sparingly:

| Event | Notification Type | When |
|-------|------------------|------|
| Device disconnected | System notification | After 5 seconds of disconnection |
| Device reconnected | None (silent) | - |
| Calibration recommended | System notification | After 7 days or significant accuracy drop |
| Error requiring action | System notification + tray icon change | Immediately |
| Training complete | None (tray icon flashes briefly) | - |

The user can disable all notifications in settings.

---

## 12. Performance Requirements

### 12.1 Latency Requirements

| Metric | Requirement | Target | Rationale |
|--------|-------------|--------|-----------|
| Feature extraction latency | < 10ms | 3ms | Must keep pace with feature rate |
| IPC round-trip latency | < 5ms | 1ms | Minimal overhead on critical path |
| Decoder inference latency | < 20ms | 8ms | Largest contributor; acceptable |
| HID emission latency | < 2ms | 0.5ms | Near-instantaneous |
| **Total intent-to-action latency** | < 100ms | 50ms | Human perception threshold |

The 100ms requirement is based on research showing that delays above 100ms are consciously perceived and feel "laggy." Our 50ms target provides margin for variability.

### 12.2 Throughput Requirements

| Metric | Requirement | Rationale |
|--------|-------------|-----------|
| Sample processing rate | 128 Hz sustained | Match device output |
| Feature extraction rate | 20-60 Hz | Configurable based on hardware |
| Action emission rate | Up to 60 Hz | Match feature rate for continuous control |
| IPC message rate | Up to 120 messages/sec | Features + actions + overhead |

### 12.3 Resource Requirements

**Memory**:

- Rust service: < 100 MB typical, < 200 MB peak
- Python process: < 500 MB typical, < 1 GB during training

**CPU**:

- Rust service: < 5% of one core during normal operation
- Python process: < 20% of one core for inference, up to 100% during training

**Disk**:

- Installation: < 200 MB
- Profile data: < 50 MB per profile
- Logs: < 100 MB (with rotation)

**GPU**:

- Not required; CPU inference is fast enough for our network size
- GPU can accelerate training if available (detected automatically by PyTorch)

### 12.4 Reliability Requirements

| Metric | Requirement |
|--------|-------------|
| Mean time between failures | > 8 hours of active use |
| Crash recovery time | < 10 seconds |
| Data loss on crash | None (profiles are saved atomically) |
| Device reconnection time | < 30 seconds |

### 12.5 Platform Compatibility

**Operating Systems**:

- Linux: Ubuntu 22.04+, Fedora 38+, Debian 12+ (other distros may work)
- Windows: Windows 10 version 1903+, Windows 11
- macOS: macOS 12 (Monterey)+

**Desktop Environments**:

- Linux: X11 (full support), Wayland (partial support, some features limited)
- Windows: Standard desktop
- macOS: Standard desktop

---

## 13. Testing Strategy

### 13.1 Testing Pyramid

Our testing strategy follows the testing pyramid with many unit tests, fewer integration tests, and targeted end-to-end tests:

```
                    ┌─────────────┐
                    │   E2E (5)   │  Full system with real/mock device
                   ─┴─────────────┴─
                  ┌─────────────────┐
                  │ Integration (50)│  Cross-component, IPC, file I/O
                 ─┴─────────────────┴─
                ┌───────────────────────┐
                │    Unit Tests (300+)   │  Individual functions, types, logic
               ─┴───────────────────────┴─
```

### 13.2 Unit Testing

**Rust unit tests** cover type validation and serialization, signal processing algorithms, configuration parsing, filter implementations, and error handling paths.

Tests are colocated with code in `#[cfg(test)]` modules and run with `cargo test`.

**Python unit tests** cover feature extraction, classifier training and prediction, decoder network forward pass, action sampling, and IPC message parsing.

Tests use pytest and are located in `tests/` directories within each module.

### 13.3 Integration Testing

Integration tests verify correct interaction between components:

**IPC integration tests** involve the Rust server and Python client communicating, message serialization round-trips, connection lifecycle handling, and error recovery.

**Storage integration tests** cover profile creation and retrieval, encryption and decryption, keychain interaction (mocked on CI), and file permission handling.

**Signal pipeline integration tests** address sample input to feature output, filter chain correctness, and timing under load.

### 13.4 End-to-End Testing

E2E tests verify the complete system with the mock device (no hardware required), full pipeline from sample generation to HID emission (captured rather than emitted), calibration flow completion, profile persistence across restarts, and error injection and recovery.

E2E tests run in CI using a headless configuration with mocked platform layer.

### 13.5 Performance Testing

**Latency benchmarks** are run on each commit, measuring feature extraction time (P50, P95, P99), IPC round-trip time, decoder inference time, and end-to-end latency.

Results are compared against baselines; regressions > 20% fail the build.

**Load testing** verifies sustained operation at maximum throughput for 1 hour without memory leaks, increasing latency, or crashes.

### 13.6 Manual Testing Checklist

Before each release, manual testing covers fresh installation on each platform, first-run experience, calibration with real hardware, extended use session (1+ hour), device disconnection and reconnection, permission edge cases, and upgrade from previous version.

---

## 14. Deployment and Distribution

### 14.1 Build Artifacts

**Linux**:

- `.deb` package for Debian/Ubuntu
- `.rpm` package for Fedora/RHEL
- AppImage for universal distribution
- Tarball for manual installation

**Windows**:

- MSI installer (recommended)
- Portable ZIP for no-install use

**macOS**:

- DMG with signed application bundle
- Homebrew cask (future)

### 14.2 Build Process

Builds are automated using GitHub Actions with a matrix across target platforms. The build process compiles Rust code with `--release` and LTO, bundles the Python environment with PyInstaller or similar, creates platform-specific installers using cargo-deb/cargo-rpm/wix/create-dmg, signs artifacts using platform-specific code signing, and uploads to release assets.

### 14.3 Code Signing

**Windows**: Authenticode signing with EV certificate (reduces SmartScreen warnings).

**macOS**: Apple Developer ID signing + notarization (required for Gatekeeper).

**Linux**: GPG-signed packages, checksums in release notes.

### 14.4 Auto-Update

The MVP does not include auto-update. Users are notified of updates through the GitHub releases page and optional email notification (opt-in during first run).

Future versions may include Sparkle-based (macOS) or Squirrel-based (Windows) auto-update.

### 14.5 Telemetry

No telemetry is collected. The application makes no network connections other than to localhost for the vendor device service.

---

## 15. Risk Assessment

### 15.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| ErrP detection accuracy too low with consumer hardware | Medium | High | Extensive signal processing, fallback to periodic explicit feedback |
| Cross-platform HID emission issues | Medium | Medium | Extensive platform-specific testing, fallback mechanisms |
| Latency exceeds requirements on low-end hardware | Low | Medium | Profiling, optimization, minimum specs documentation |
| Python dependency conflicts | Medium | Low | Pinned versions, virtual environment isolation |

### 15.2 Product Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Calibration too time-consuming for users | Medium | High | Streamlined games, progress saving, quality shortcuts |
| User frustration during learning period | High | Medium | Clear expectations, encouraging feedback, visible progress |
| Device availability changes | Low | High | Abstraction layer, multiple device support |

### 15.3 Security Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Brain data exfiltration | Very Low | Critical | No network connections, encrypted storage |
| Malicious model injection | Very Low | High | Signed models, integrity verification |
| Credential theft | Low | Medium | Platform keychain, no plaintext storage |

### 15.4 Regulatory Risks

EEG-based consumer products occupy a gray area in medical device regulation. NeuroHID is positioned as an accessibility/input device, not a medical device. We must avoid medical claims in marketing, include appropriate disclaimers, and monitor regulatory developments in target markets.

---

## 16. Development Roadmap

### 16.1 Phase 0: Foundation (Weeks 1-3)

**Week 1**: Rust fundamentals learning, project structure setup, CI/CD pipeline.

**Week 2**: Async Rust and Tokio, WebSocket basics, initial crate definitions.

**Week 3**: Core type system complete, mock device implementation, storage layer.

Milestone: Rust codebase compiles, mock device streams data, storage works.

### 16.2 Phase 1: Core Infrastructure (Weeks 4-7)

**Week 4**: Emotiv Cortex adapter, real device connectivity, cross-platform testing.

**Week 5**: Signal processing pipeline, filters, feature extraction, benchmarking.

**Week 6**: Platform layer implementation, HID emission on all platforms.

**Week 7**: Main event loop, service orchestration, configuration loading.

Milestone: Full Rust service runs, processes real EEG, emits HID events.

### 16.3 Phase 2: ML Integration (Weeks 8-10)

**Week 8**: Python IPC client, message passing, latency testing.

**Week 9**: Decoder implementation, PPO training loop, action generation.

**Week 10**: ErrP detector, classifier training, reward signal integration.

Milestone: Full pipeline operational with ML decoding and ErrP-based learning.

### 16.4 Phase 3: Calibration (Weeks 11-14)

**Week 11**: egui basics, window management, game UI framework.

**Week 12**: Grid Maze game implementation, trial recording, error injection.

**Week 13**: Target Tracking game, continuous control calibration.

**Week 14**: Calibration wizard, quality metrics, profile creation.

Milestone: Complete calibration flow, new users can calibrate from scratch.

### 16.5 Phase 4: Polish (Weeks 15-17)

**Week 15**: End-to-end testing, edge case handling, error recovery.

**Week 16**: System tray application, status UI, settings interface.

**Week 17**: Installer creation, documentation, fresh system testing.

Milestone: MVP ready for alpha testing.

### 16.6 Post-MVP Roadmap

**v0.2**: Multi-profile support, profile sharing, improved calibration.

**v0.3**: Additional device support (OpenBCI, Muse), extended action vocabulary.

**v0.4**: Advanced signal processing (ICA artifact removal, source localization).

**v0.5**: Mobile companion app for remote status monitoring.

**v1.0**: Production release with auto-update, comprehensive documentation, community support.

---

## 17. Glossary

**Action**: A discrete or continuous output from the decoder, representing the user's intended input (mouse movement, click, keystroke).

**Anterior Cingulate Cortex (ACC)**: Brain region involved in error monitoring and cognitive control, primary source of the Error-Related Negativity (ERN).

**Artifact**: Unwanted signal in EEG recording, such as eye blinks, muscle activity, or electrode movement.

**Band Power**: The total energy in a specific frequency range of the EEG signal, computed from the power spectral density.

**BCI (Brain-Computer Interface)**: A system that translates brain signals into commands for external devices.

**Calibration**: The process of training user-specific models for ErrP detection and initial decoder parameters.

**Confidence**: The decoder's estimate of how certain it is about the intended action, used to filter uncertain outputs.

**Cortex API**: Emotiv's software interface for accessing EEG data from their devices.

**Debounce**: A technique to prevent multiple rapid triggerings of a discrete action, enforcing a minimum interval between activations.

**Decoder**: The neural network that maps EEG features to intended actions.

**EEG (Electroencephalography)**: The recording of electrical activity along the scalp, produced by neuronal firing in the brain.

**Emotiv Insight**: A 5-channel consumer EEG headset used as the primary target device for NeuroHID.

**ERN (Error-Related Negativity)**: A negative voltage deflection in EEG occurring 50-100ms after error perception, part of the ErrP.

**ERP (Event-Related Potential)**: A measured brain response that is the direct result of a specific sensory, cognitive, or motor event.

**ErrP (Error-Related Potential)**: An ERP generated when a person perceives an error, used by NeuroHID as an implicit reward signal.

**Feature Extraction**: The process of computing meaningful numerical representations from raw EEG signals.

**GAE (Generalized Advantage Estimation)**: A technique for computing advantage estimates in policy gradient methods, balancing bias and variance.

**HID (Human Interface Device)**: A type of computer device that interacts directly with humans, such as keyboards, mice, and game controllers.

**IPC (Inter-Process Communication)**: Mechanisms for processes to communicate and synchronize, such as sockets and shared memory.

**Latency**: The delay between an event (such as user intention) and the system's response (such as cursor movement).

**Online Learning**: Machine learning that updates the model continuously during use, rather than training only in a separate phase.

**Pe (Error Positivity)**: A positive voltage deflection following the ERN, peaking 200-400ms after error perception.

**PPO (Proximal Policy Optimization)**: A policy gradient reinforcement learning algorithm that constrains policy updates for stability.

**Profile**: A collection of user-specific data including calibration data, trained models, and preferences.

**Reward Signal**: In reinforcement learning, the signal that indicates how good an action was, used to update the policy.

**Ring Buffer**: A fixed-size buffer that overwrites oldest data when full, used for maintaining a sliding window of recent samples.

**Smoothing**: Processing applied to decoded actions to reduce jitter and produce more natural-feeling movement.

**uinput**: A Linux kernel module that allows userspace programs to create virtual input devices.

---

## 18. References

### Research Papers

Chavarriaga, R., Sobolewski, A., &amp; Millán, J. D. R. (2014). Errare machinale est: the use of error-related potentials in brain-machine interfaces. *Frontiers in Neuroscience*, 8, 208.

Iturrate, I., Chavarriaga, R., Montesano, L., Minguez, J., &amp; Millán, J. D. R. (2015). Teaching brain-machine interfaces as an alternative paradigm to neuroprosthetics control. *Scientific Reports*, 5, 13893.

Kreilinger, A., Neuper, C., &amp; Müller-Putz, G. R. (2012). Error potential detection during continuous movement of an artificial arm controlled by brain-computer interface. *Medical &amp; Biological Engineering &amp; Computing*, 50(3), 223-230.

Schulman, J., Wolski, F., Dhariwal, P., Radford, A., &amp; Klimov, O. (2017). Proximal policy optimization algorithms. *arXiv preprint arXiv:1707.06347*.

### Technical Documentation

Emotiv Cortex API Documentation: <https://emotiv.gitbook.io/cortex-api/>

PyTorch Documentation: <https://pytorch.org/docs/>

Tokio Async Runtime: <https://tokio.rs/>

egui Immediate Mode GUI: <https://docs.rs/egui/>

### Standards

USB HID Usage Tables: <https://usb.org/sites/default/files/hut1_3_0.pdf>

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1.1-draft | February 2026 | Jon | Add Observation space extension to non-goals for MVP |
| 0.1.0-draft | January 2026 | Claude | Initial specification |

---

*This document is a living specification and will be updated as the project evolves. Feedback and contributions are welcome.*

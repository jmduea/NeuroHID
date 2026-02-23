//! Python bindings for the `neurohid-signal` processing pipeline.

use neurohid_signal::buffer::{BufferConfig, SampleBuffer, SignalWindow};
use neurohid_signal::features::{FeatureConfig, FeatureExtractor, TemporalState};
use neurohid_signal::filter::{FilterChain, FilterConfig, FilterType};
use neurohid_signal::pipeline::{PipelineConfig, PipelineStats, SignalPipeline};
use neurohid_types::error::SignalError;
use numpy::{PyArray1, PyArray2, PyArrayMethods};
use pyo3::prelude::*;

use crate::types::PyFeatureVector;

/// Convert `SignalError` → `PyErr` using the Python-side `SignalError` exception.
fn sig_err(e: SignalError) -> PyErr {
    crate::errors::to_py_err(neurohid_types::error::Error::Signal(e))
}

// ---------------------------------------------------------------------------
// FilterType
// ---------------------------------------------------------------------------

/// DSP filter specification.
#[pyclass(name = "FilterType", from_py_object)]
#[derive(Clone)]
pub struct PyFilterType {
    pub inner: FilterType,
}

#[pymethods]
impl PyFilterType {
    #[staticmethod]
    fn lowpass(cutoff_hz: f32) -> Self {
        Self {
            inner: FilterType::Lowpass { cutoff_hz },
        }
    }
    #[staticmethod]
    fn highpass(cutoff_hz: f32) -> Self {
        Self {
            inner: FilterType::Highpass { cutoff_hz },
        }
    }
    #[staticmethod]
    fn bandpass(low_hz: f32, high_hz: f32) -> Self {
        Self {
            inner: FilterType::Bandpass { low_hz, high_hz },
        }
    }
    #[staticmethod]
    fn notch(center_hz: f32, q_factor: f32) -> Self {
        Self {
            inner: FilterType::Notch {
                center_hz,
                q_factor,
            },
        }
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

// ---------------------------------------------------------------------------
// FilterConfig
// ---------------------------------------------------------------------------

/// Configuration for a filter chain.
#[pyclass(name = "FilterConfig", from_py_object)]
#[derive(Clone)]
pub struct PyFilterConfig {
    pub inner: FilterConfig,
}

#[pymethods]
impl PyFilterConfig {
    #[new]
    fn new(filters: Vec<PyFilterType>, sample_rate_hz: f32) -> Self {
        Self {
            inner: FilterConfig {
                filters: filters.into_iter().map(|f| f.inner).collect(),
                sample_rate_hz,
            },
        }
    }

    #[staticmethod]
    fn eeg_default(sample_rate_hz: f32, line_freq_hz: f32) -> Self {
        Self {
            inner: FilterConfig::eeg_default(sample_rate_hz, line_freq_hz),
        }
    }

    #[getter]
    fn sample_rate_hz(&self) -> f32 {
        self.inner.sample_rate_hz
    }

    fn __repr__(&self) -> String {
        format!(
            "FilterConfig(filters={}, rate={}Hz)",
            self.inner.filters.len(),
            self.inner.sample_rate_hz
        )
    }
}

// ---------------------------------------------------------------------------
// FilterChain
// ---------------------------------------------------------------------------

/// Real-time digital filter chain (biquad cascade per channel).
#[pyclass(name = "FilterChain")]
pub struct PyFilterChain {
    inner: FilterChain,
}

#[pymethods]
impl PyFilterChain {
    #[new]
    fn new(config: PyFilterConfig, channel_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: FilterChain::new(config.inner, channel_count).map_err(sig_err)?,
        })
    }

    #[staticmethod]
    fn eeg_default(sample_rate_hz: f32, channel_count: usize, line_freq_hz: f32) -> PyResult<Self> {
        Ok(Self {
            inner: FilterChain::eeg_default(sample_rate_hz, channel_count, line_freq_hz)
                .map_err(sig_err)?,
        })
    }

    /// Process a single multi-channel sample in-place. Returns filtered values.
    fn process_sample<'py>(
        &mut self,
        py: Python<'py>,
        sample: Vec<f32>,
    ) -> PyResult<Bound<'py, PyArray1<f32>>> {
        let result = self.inner.process_sample(&sample).map_err(sig_err)?;
        Ok(PyArray1::from_vec(py, result))
    }

    fn reset(&mut self) {
        self.inner.reset();
    }
}

// ---------------------------------------------------------------------------
// BufferConfig
// ---------------------------------------------------------------------------

/// Ring-buffer configuration.
#[pyclass(name = "BufferConfig", from_py_object)]
#[derive(Clone)]
pub struct PyBufferConfig {
    pub inner: BufferConfig,
}

#[pymethods]
impl PyBufferConfig {
    #[new]
    #[pyo3(signature = (capacity_samples = 1024, channel_count = 5))]
    fn new(capacity_samples: usize, channel_count: usize) -> Self {
        Self {
            inner: BufferConfig {
                capacity_samples,
                channel_count,
            },
        }
    }

    #[getter]
    fn capacity_samples(&self) -> usize {
        self.inner.capacity_samples
    }
    #[getter]
    fn channel_count(&self) -> usize {
        self.inner.channel_count
    }

    fn __repr__(&self) -> String {
        format!(
            "BufferConfig(capacity={}, channels={})",
            self.inner.capacity_samples, self.inner.channel_count
        )
    }
}

// ---------------------------------------------------------------------------
// SampleBuffer
// ---------------------------------------------------------------------------

/// Columnar ring buffer for multi-channel EEG data.
#[pyclass(name = "SampleBuffer")]
pub struct PySampleBuffer {
    inner: SampleBuffer,
}

#[pymethods]
impl PySampleBuffer {
    #[new]
    fn new(config: PyBufferConfig) -> Self {
        Self {
            inner: SampleBuffer::new(config.inner),
        }
    }

    fn push(&mut self, values: Vec<f32>, timestamp: i64) -> PyResult<()> {
        self.inner.push(&values, timestamp).map_err(sig_err)
    }

    /// Return the last `num_samples` as a `SignalWindow`, or `None`.
    fn window(&self, num_samples: usize) -> Option<PySignalWindow> {
        self.inner
            .window(num_samples)
            .map(|w| PySignalWindow { inner: w })
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    fn total_pushed(&self) -> u64 {
        self.inner.total_pushed()
    }
    fn clear(&mut self) {
        self.inner.clear();
    }
}

// ---------------------------------------------------------------------------
// SignalWindow
// ---------------------------------------------------------------------------

/// A snapshot of buffered signal data.
#[pyclass(name = "SignalWindow", skip_from_py_object)]
#[derive(Clone)]
pub struct PySignalWindow {
    pub inner: SignalWindow,
}

#[pymethods]
impl PySignalWindow {
    /// Channel data as a 2-D numpy array `[channels, samples]`.
    #[getter]
    fn channel_data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let rows = self.inner.channel_count;
        let cols = self.inner.sample_count;
        let flat: Vec<f32> = self
            .inner
            .channel_data
            .iter()
            .flat_map(|ch| ch.iter().copied())
            .collect();
        PyArray1::from_vec(py, flat).reshape([rows, cols])
    }

    /// Timestamps as 1-D numpy array (microseconds since epoch).
    #[getter]
    fn timestamps<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<i64>> {
        PyArray1::from_slice(py, &self.inner.timestamps)
    }

    #[getter]
    fn channel_count(&self) -> usize {
        self.inner.channel_count
    }
    #[getter]
    fn sample_count(&self) -> usize {
        self.inner.sample_count
    }

    /// Get a single channel as a 1-D numpy array.
    fn channel<'py>(&self, py: Python<'py>, ch: usize) -> PyResult<Bound<'py, PyArray1<f32>>> {
        self.inner
            .channel(ch)
            .map(|data| PyArray1::from_slice(py, data))
            .ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err(format!("channel index {ch} out of range"))
            })
    }

    fn duration_secs(&self, sample_rate_hz: f32) -> f32 {
        self.inner.duration_secs(sample_rate_hz)
    }

    fn has_minimum(&self, min_samples: usize) -> bool {
        self.inner.has_minimum(min_samples)
    }

    fn __repr__(&self) -> String {
        format!(
            "SignalWindow(channels={}, samples={})",
            self.inner.channel_count, self.inner.sample_count
        )
    }
}

// ---------------------------------------------------------------------------
// FeatureConfig
// ---------------------------------------------------------------------------

/// Feature extraction configuration.
#[pyclass(name = "FeatureConfig", from_py_object)]
#[derive(Clone)]
pub struct PyFeatureConfig {
    pub inner: FeatureConfig,
}

#[pymethods]
impl PyFeatureConfig {
    #[new]
    #[pyo3(signature = (
        sample_rate_hz = 128.0,
        channel_count = 5,
        welch_segment_len = 64,
        welch_overlap = 0.5,
        frontal_pair = (0, 1),
        emit_labels = false,
    ))]
    fn new(
        sample_rate_hz: f32,
        channel_count: usize,
        welch_segment_len: usize,
        welch_overlap: f32,
        frontal_pair: (usize, usize),
        emit_labels: bool,
    ) -> Self {
        Self {
            inner: FeatureConfig {
                sample_rate_hz,
                channel_count,
                welch_segment_len,
                welch_overlap,
                frontal_pair,
                emit_labels,
            },
        }
    }

    #[getter]
    fn sample_rate_hz(&self) -> f32 {
        self.inner.sample_rate_hz
    }
    #[getter]
    fn channel_count(&self) -> usize {
        self.inner.channel_count
    }
    #[getter]
    fn welch_segment_len(&self) -> usize {
        self.inner.welch_segment_len
    }
    #[getter]
    fn welch_overlap(&self) -> f32 {
        self.inner.welch_overlap
    }
    #[getter]
    fn emit_labels(&self) -> bool {
        self.inner.emit_labels
    }

    fn __repr__(&self) -> String {
        format!(
            "FeatureConfig(rate={}Hz, channels={})",
            self.inner.sample_rate_hz, self.inner.channel_count
        )
    }
}

// ---------------------------------------------------------------------------
// FeatureExtractor
// ---------------------------------------------------------------------------

/// Extracts 180-dimensional feature vectors from signal windows.
#[pyclass(name = "FeatureExtractor")]
pub struct PyFeatureExtractor {
    inner: FeatureExtractor,
}

#[pymethods]
impl PyFeatureExtractor {
    #[new]
    fn new(config: PyFeatureConfig) -> Self {
        Self {
            inner: FeatureExtractor::new(config.inner),
        }
    }

    fn feature_dim(&self) -> usize {
        self.inner.feature_dim()
    }

    fn extract(&mut self, window: &PySignalWindow) -> PyResult<PyFeatureVector> {
        let fv = self.inner.extract(&window.inner).map_err(sig_err)?;
        Ok(PyFeatureVector { inner: fv })
    }

    /// Extract features with temporal context. Returns `(feature_vector, channel_psds)`.
    fn extract_with_temporal<'py>(
        &mut self,
        py: Python<'py>,
        window: &PySignalWindow,
        temporal: Option<&PyTemporalState>,
    ) -> PyResult<(PyFeatureVector, Bound<'py, PyArray2<f32>>)> {
        let (fv, channel_psds) = self
            .inner
            .extract_with_temporal(&window.inner, temporal.map(|t| &t.inner))
            .map_err(sig_err)?;
        // channel_psds: Vec<Vec<f32>> → numpy 2-D array
        let rows = channel_psds.len();
        let cols = channel_psds.first().map_or(0, |r| r.len());
        let flat: Vec<f32> = channel_psds
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect();
        let arr = PyArray1::from_vec(py, flat).reshape([rows, cols])?;
        Ok((PyFeatureVector { inner: fv }, arr))
    }
}

// ---------------------------------------------------------------------------
// TemporalState
// ---------------------------------------------------------------------------

/// Exponential moving average state for temporal feature tracking.
#[pyclass(name = "TemporalState")]
pub struct PyTemporalState {
    pub inner: TemporalState,
}

#[pymethods]
impl PyTemporalState {
    #[new]
    fn new(channel_count: usize, extraction_rate_hz: f32) -> Self {
        Self {
            inner: TemporalState::new(channel_count, extraction_rate_hz),
        }
    }

    fn update(&mut self, band_powers: Vec<Vec<f32>>) {
        self.inner.update(&band_powers);
    }

    fn update_count(&self) -> u64 {
        self.inner.update_count()
    }

    fn __repr__(&self) -> String {
        format!("TemporalState(updates={})", self.inner.update_count())
    }
}

// ---------------------------------------------------------------------------
// PipelineConfig
// ---------------------------------------------------------------------------

/// Full signal-pipeline configuration.
#[pyclass(name = "PipelineConfig", from_py_object)]
#[derive(Clone)]
pub struct PyPipelineConfig {
    pub inner: PipelineConfig,
}

#[pymethods]
impl PyPipelineConfig {
    /// Create a default pipeline config.
    #[new]
    fn new() -> Self {
        Self {
            inner: PipelineConfig::default(),
        }
    }

    /// Pre-built config tuned for the Emotiv Insight headset.
    #[staticmethod]
    #[pyo3(signature = (line_freq_hz = 60.0))]
    fn emotiv_insight(line_freq_hz: f32) -> Self {
        Self {
            inner: PipelineConfig::emotiv_insight(line_freq_hz),
        }
    }

    #[getter]
    fn artifact_threshold_uv(&self) -> f32 {
        self.inner.artifact_threshold_uv
    }
    #[getter]
    fn window_samples(&self) -> usize {
        self.inner.window_samples
    }
    #[getter]
    fn step_samples(&self) -> usize {
        self.inner.step_samples
    }
    #[getter]
    fn zscore_window_secs(&self) -> f32 {
        self.inner.zscore_window_secs
    }

    fn __repr__(&self) -> String {
        format!(
            "PipelineConfig(window={}, step={}, artifact_thresh={}µV)",
            self.inner.window_samples, self.inner.step_samples, self.inner.artifact_threshold_uv
        )
    }
}

// ---------------------------------------------------------------------------
// PipelineStats
// ---------------------------------------------------------------------------

/// Pipeline throughput statistics.
#[pyclass(name = "PipelineStats", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPipelineStats {
    pub inner: PipelineStats,
}

#[pymethods]
impl PyPipelineStats {
    #[getter]
    fn samples_received(&self) -> u64 {
        self.inner.samples_received
    }
    #[getter]
    fn samples_rejected(&self) -> u64 {
        self.inner.samples_rejected
    }
    #[getter]
    fn features_produced(&self) -> u64 {
        self.inner.features_produced
    }

    fn rejection_rate(&self) -> f32 {
        self.inner.rejection_rate()
    }

    fn __repr__(&self) -> String {
        format!(
            "PipelineStats(received={}, rejected={}, features={})",
            self.inner.samples_received, self.inner.samples_rejected, self.inner.features_produced
        )
    }
}

// ---------------------------------------------------------------------------
// SignalPipeline
// ---------------------------------------------------------------------------

/// End-to-end signal processing pipeline: filter → buffer → extract.
#[pyclass(name = "SignalPipeline")]
pub struct PySignalPipeline {
    inner: SignalPipeline,
}

#[pymethods]
impl PySignalPipeline {
    #[new]
    fn new(config: PyPipelineConfig) -> PyResult<Self> {
        Ok(Self {
            inner: SignalPipeline::new(config.inner).map_err(sig_err)?,
        })
    }

    /// Push a raw multi-channel sample into the pipeline.
    fn push_sample(&mut self, values: Vec<f32>, timestamp: i64) -> PyResult<()> {
        self.inner.push_sample(&values, timestamp).map_err(sig_err)
    }

    /// Try to extract a feature vector. Returns `None` if not enough new samples.
    fn try_extract(&mut self) -> PyResult<Option<PyFeatureVector>> {
        self.inner
            .try_extract()
            .map(|opt| opt.map(|fv| PyFeatureVector { inner: fv }))
            .map_err(sig_err)
    }

    /// Capture a signal window of the last `num_samples` from the internal buffer.
    fn capture_window(&self, num_samples: usize) -> Option<PySignalWindow> {
        self.inner
            .capture_window(num_samples)
            .map(|w| PySignalWindow { inner: w })
    }

    fn stats(&self) -> PyPipelineStats {
        PyPipelineStats {
            inner: self.inner.stats().clone(),
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn buffer_len(&self) -> usize {
        self.inner.buffer_len()
    }

    fn feature_dim(&self) -> usize {
        self.inner.feature_dim()
    }

    fn __repr__(&self) -> String {
        let s = self.inner.stats();
        format!(
            "SignalPipeline(buffer={}, features_produced={})",
            self.inner.buffer_len(),
            s.features_produced
        )
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFilterType>()?;
    m.add_class::<PyFilterConfig>()?;
    m.add_class::<PyFilterChain>()?;
    m.add_class::<PyBufferConfig>()?;
    m.add_class::<PySampleBuffer>()?;
    m.add_class::<PySignalWindow>()?;
    m.add_class::<PyFeatureConfig>()?;
    m.add_class::<PyFeatureExtractor>()?;
    m.add_class::<PyTemporalState>()?;
    m.add_class::<PyPipelineConfig>()?;
    m.add_class::<PyPipelineStats>()?;
    m.add_class::<PySignalPipeline>()?;
    Ok(())
}

//! Controller statistics used for reporting and flow control.
//!
//! # Design
//!
//! We use a lock-free design to avoid blocking the datapath.  All
//! performance counters are implemented as atomics.  However, updates
//! across multiple counters are not atomic.  More importantly, performance
//! counters can be slightly out of sync with the actual state of the
//! pipeline.  Such discrepancies are bounded and do _not_ accumulate
//! over time.
//!
//! # Example
//!
//! Consider the following interleaving of three threads: transport
//! endpoint 1, transport endpoint 2, and circuit thread:
// The quotes below are needed because of
// https://github.com/rust-lang/rustfmt/issues/4210
//! ```text
//! | Thread        | Action                              | `buffered_input_records` | actual # of buffered records |
//! | ------------- | ----------------------------------- | :----------------------: | :--------------------------: |
//! | 1. endpoint 1 | enqueues 10 records                 |            10            |              10              |
//! | 2. circuit:   | `ControllerStatus::consume_counters`|             0            |              10              |
//! | 3. endpoint 2 | enqueue 10 records                  |            10            |              20              |
//! | 4. circuit    | circuit.step()                      |            10            |              0               |
//! ```
//!
//! Here endpoint 2 buffers additional records after the
//! `ControllerStatus::buffered_input_records` counter is reset to zero.  As a
//! result, all 20 records enqueued by both endpoints are processed
//! by the circuit, but the counter shows that 10 records are still
//! pending.

use super::{EndpointId, GlobalControllerConfig, InputEndpointConfig, OutputEndpointConfig};
use anyhow::Error as AnyError;
use crossbeam::sync::{ShardedLock, ShardedLockReadGuard, Unparker};
use serde::{Serialize, Serializer};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

#[derive(Default, Serialize)]
pub struct GlobalControllerMetrics {
    /// Total number of records buffered by all endpoints.
    pub buffered_input_records: AtomicU64,
}

type InputsStatus = ShardedLock<BTreeMap<EndpointId, InputEndpointStatus>>;
type OutputsStatus = ShardedLock<BTreeMap<EndpointId, OutputEndpointStatus>>;

// Serialize inputs as a vector of `InputEndpointStatus`.
fn serialize_inputs<S>(inputs: &InputsStatus, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let inputs = inputs.read().unwrap();
    let mut inputs = inputs.values().collect::<Vec<_>>();
    inputs.sort_by(|ep1, ep2| ep1.endpoint_name.cmp(&ep2.endpoint_name));
    inputs.serialize(serializer)
}

// Serialize outputs as a vector of `InputEndpointStatus`.
fn serialize_outputs<S>(outputs: &OutputsStatus, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let outputs = outputs.read().unwrap();
    let mut outputs = outputs.values().collect::<Vec<_>>();
    outputs.sort_by(|ep1, ep2| ep1.endpoint_name.cmp(&ep2.endpoint_name));
    outputs.serialize(serializer)
}

/// Controller statistics.
#[derive(Serialize)]
pub struct ControllerStatus {
    /// Global controller configuration.
    pub global_config: GlobalControllerConfig,

    /// Global controller metrics.
    pub global_metrics: GlobalControllerMetrics,

    /// Input endpoint configs and metrics.
    // `ShardedLock` is a read/write lock optimized for fast reads.
    // Write access is only required when adding or removing an endpoint.
    // Regular stats updates only require a read lock thanks to the use of
    // atomics.
    #[serde(serialize_with = "serialize_inputs")]
    inputs: InputsStatus,

    /// Output endpoint configs and metrics.
    #[serde(serialize_with = "serialize_outputs")]
    outputs: OutputsStatus,
}

impl ControllerStatus {
    pub fn new(global_config: &GlobalControllerConfig) -> Self {
        Self {
            global_config: global_config.clone(),
            global_metrics: Default::default(),
            inputs: ShardedLock::new(BTreeMap::new()),
            outputs: ShardedLock::new(BTreeMap::new()),
        }
    }

    /// Initialize stats for a new input endpoint.
    pub fn add_input(
        &self,
        endpoint_id: &EndpointId,
        endpoint_name: &str,
        config: &InputEndpointConfig,
    ) {
        self.inputs.write().unwrap().insert(
            *endpoint_id,
            InputEndpointStatus::new(endpoint_name, config),
        );
    }

    /// Initialize stats for a new output endpoint.
    pub fn add_output(
        &self,
        endpoint_id: &EndpointId,
        endpoint_name: &str,
        config: &OutputEndpointConfig,
    ) {
        self.outputs.write().unwrap().insert(
            *endpoint_id,
            OutputEndpointStatus::new(endpoint_name, config),
        );
    }

    /// Total number of records buffered by all input endpoints.
    pub fn num_buffered_input_records(&self) -> u64 {
        self.global_metrics
            .buffered_input_records
            .load(Ordering::Acquire)
    }

    /// Input endpoint stats.
    pub fn input_status(&self) -> ShardedLockReadGuard<BTreeMap<EndpointId, InputEndpointStatus>> {
        self.inputs.read().unwrap()
    }

    /// Output endpoint stats.
    pub fn output_status(
        &self,
    ) -> ShardedLockReadGuard<BTreeMap<EndpointId, OutputEndpointStatus>> {
        self.outputs.read().unwrap()
    }

    /// Number of records buffered by the endpoint or 0 if the endpoint
    /// doesn't exist (the latter is possible if the endpoint is being
    /// destroyed).
    pub fn num_input_endpoint_buffered_records(&self, endpoint_id: &EndpointId) -> u64 {
        match &self.inputs.read().unwrap().get(endpoint_id) {
            None => 0,
            Some(endpoint_stats) => endpoint_stats
                .metrics
                .buffered_records
                .load(Ordering::Acquire),
        }
    }

    /// Reset all buffered record and byte counters to zero.
    ///
    /// This method is invoked before `DBSPHandle::step` to indicate that all
    /// buffered data is about to be consumed.  See module-level documentation
    /// for details.
    pub fn consume_buffered_inputs(&self) {
        self.global_metrics
            .buffered_input_records
            .store(0, Ordering::Release);
        for endpoint_stats in self.inputs.read().unwrap().values() {
            endpoint_stats.consume_buffered();
        }
    }

    /// True if the number of records buffered by the endpoint exceeds
    /// its `max_buffered_records` config parameter.
    pub fn input_endpoint_full(&self, endpoint_id: &EndpointId) -> bool {
        let buffered_records = self.num_input_endpoint_buffered_records(endpoint_id);

        let max_buffered_records = match self.inputs.read().unwrap().get(endpoint_id) {
            None => return false,
            Some(endpoint) => endpoint.config.max_buffered_records,
        };

        buffered_records >= max_buffered_records
    }

    /// Update counters after receiving a new input batch.
    ///
    /// # Arguments
    ///
    /// * `endpoint_id` - id of the input endpoint.
    /// * `num_bytes` - number of bytes received.
    /// * `num_records` - number of records in the deserialized batch.
    /// * `global_config` - global controller config.
    /// * `circuit_thread_unparker` - unparker used to wake up the circuit
    ///   thread if the total number of buffered records exceeds
    ///   `min_batch_size_records`.
    /// * `backpressure_thread_unparker` - unparker used to wake up the the
    ///   backpressure thread if the endpoint is full.
    pub fn input_batch(
        &self,
        endpoint_id: EndpointId,
        num_bytes: usize,
        num_records: usize,
        global_config: &GlobalControllerConfig,
        circuit_thread_unparker: &Unparker,
        backpressure_thread_unparker: &Unparker,
    ) {
        let num_records = num_records as u64;
        let num_bytes = num_bytes as u64;

        // Increment buffered_records; unpark circuit thread once
        // `min_batch_size_records` is exceeded.
        let old = self
            .global_metrics
            .buffered_input_records
            .fetch_add(num_records, Ordering::AcqRel);

        if old == 0
            || (old <= global_config.min_batch_size_records
                && old + num_records > global_config.min_batch_size_records)
        {
            circuit_thread_unparker.unpark();
        }

        let inputs = self.inputs.read().unwrap();

        // Update endpoint counters; unpark backpressure thread if endpoint's
        // `max_buffered_records` exceeded.
        //
        // There is a potential race condition if the endpoint is currently being
        // removed. In this case, it's safe to ignore this operation.
        if let Some(endpoint_stats) = inputs.get(&endpoint_id) {
            let old = endpoint_stats.add_buffered(num_bytes, num_records);

            if old < endpoint_stats.config.max_buffered_records
                && old + num_records >= endpoint_stats.config.max_buffered_records
            {
                backpressure_thread_unparker.unpark();
            }
        };
    }

    /// Update counters after receiving an end-of-file event on an input
    /// endpoint.
    ///
    /// # Arguments
    ///
    /// * `endpoint_id` - id of the input endpoint.
    /// * `num_records` - number of records returned by `Parser::eoi`.
    /// * `global_config` - global controller config.
    /// * `circuit_thread_unparker` - unparker used to wake up the circuit
    ///   thread if the total number of buffered records exceeds
    ///   `min_batch_size_records`.
    pub fn eoi(
        &self,
        endpoint_id: EndpointId,
        num_records: usize,
        global_config: &GlobalControllerConfig,
        circuit_thread_unparker: &Unparker,
    ) {
        let num_records = num_records as u64;

        // Increment buffered_input_records; unpark circuit thread if
        // `min_batch_size_records` exceeded.
        let old = self
            .global_metrics
            .buffered_input_records
            .fetch_add(num_records, Ordering::AcqRel);
        if old < global_config.min_batch_size_records
            && old + num_records >= global_config.min_batch_size_records
        {
            circuit_thread_unparker.unpark();
        }

        // Update endpoint counters, no need to wake up the backpressure thread since we
        // won't see any more inputs from this endpoint.
        let inputs = self.inputs.read().unwrap();
        if let Some(endpoint_stats) = inputs.get(&endpoint_id) {
            endpoint_stats.add_buffered(0, num_records);
        };
    }

    pub fn enqueue_batch(&self, endpoint_id: EndpointId, num_records: usize) {
        if let Some(endpoint_stats) = self.output_status().get(&endpoint_id) {
            endpoint_stats.enqueue_batch(num_records);
        }
    }

    pub fn output_batch(
        &self,
        endpoint_id: EndpointId,
        num_records: usize,
        circuit_thread_unparker: &Unparker,
    ) {
        if let Some(endpoint_stats) = self.output_status().get(&endpoint_id) {
            let old = endpoint_stats.output_batch(num_records);
            if old < endpoint_stats.config.max_buffered_records
                && old + num_records as u64 >= endpoint_stats.config.max_buffered_records
            {
                circuit_thread_unparker.unpark();
            }
        };
    }

    pub fn output_buffer(&self, endpoint_id: EndpointId, num_bytes: usize) {
        if let Some(endpoint_stats) = self.output_status().get(&endpoint_id) {
            endpoint_stats.output_buffer(num_bytes);
        };
    }

    pub fn output_buffers_full(&self) -> bool {
        self.output_status().values().any(|endpoint_stats| {
            endpoint_stats
                .metrics
                .buffered_records
                .load(Ordering::Acquire)
                >= endpoint_stats.config.max_buffered_records
        })
    }

    pub fn input_transport_error(&self, endpoint_id: EndpointId, fatal: bool, error: &AnyError) {
        if let Some(endpoint_stats) = self.input_status().get(&endpoint_id) {
            endpoint_stats.error(fatal, error);
        }
    }

    pub fn output_transport_error(&self, endpoint_id: EndpointId, fatal: bool, error: &AnyError) {
        if let Some(endpoint_stats) = self.output_status().get(&endpoint_id) {
            endpoint_stats.error(fatal, error);
        }
    }
}

#[derive(Default, Serialize)]
pub struct InputEndpointMetrics {
    /// Total bytes pushed to the endpoint since it was created.
    pub total_bytes: AtomicU64,

    /// Total records pushed to the endpoint since it was created.
    pub total_records: AtomicU64,

    /// Number of bytes currently buffered by the endpoint
    /// (not yet consumed by the circuit).
    pub buffered_bytes: AtomicU64,

    /// Number of records currently buffered by the endpoint
    /// (not yet consumed by the circuit).
    pub buffered_records: AtomicU64,

    pub num_errors: AtomicU64,
}

/// Input endpoint status information.
#[derive(Serialize)]
pub struct InputEndpointStatus {
    pub endpoint_name: String,

    /// Endpoint configuration (doesn't change).
    pub config: InputEndpointConfig,

    /// Performance metrics.
    pub metrics: InputEndpointMetrics,

    /// The first fatal error that occurred at the endpoint.
    pub fatal_error: Mutex<Option<String>>,
}

impl InputEndpointStatus {
    fn new(endpoint_name: &str, config: &InputEndpointConfig) -> Self {
        Self {
            endpoint_name: endpoint_name.to_string(),
            config: config.clone(),
            metrics: Default::default(),
            fatal_error: Mutex::new(None),
        }
    }

    fn consume_buffered(&self) {
        self.metrics.buffered_bytes.store(0, Ordering::Release);
        self.metrics.buffered_records.store(0, Ordering::Release);
    }

    /// Increment the number of buffered bytes and records; return
    /// the previous number of buffered records.
    fn add_buffered(&self, num_bytes: u64, num_records: u64) -> u64 {
        if num_bytes > 0 {
            // We are only updating statistics here, so no need to pay for
            // strong consistency.
            self.metrics
                .total_bytes
                .fetch_add(num_bytes, Ordering::Relaxed);
            self.metrics
                .buffered_bytes
                .fetch_add(num_bytes, Ordering::AcqRel);
        }

        self.metrics
            .total_records
            .fetch_add(num_records, Ordering::Relaxed);
        self.metrics
            .buffered_records
            .fetch_add(num_records, Ordering::AcqRel)
    }

    /// Increment error counter.  If this is the first fatal error,
    /// save it in `self.fatal_error`.
    fn error(&self, fatal: bool, error: &AnyError) {
        self.metrics.num_errors.fetch_add(1, Ordering::AcqRel);
        if fatal {
            let mut fatal_error = self.fatal_error.lock().unwrap();
            if fatal_error.is_none() {
                *fatal_error = Some(error.to_string());
            }
        }
    }
}

#[derive(Default, Serialize)]
pub struct OutputEndpointMetrics {
    pub transmitted_records: AtomicU64,
    pub transmitted_bytes: AtomicU64,

    pub buffered_records: AtomicU64,
    pub buffered_batches: AtomicU64,

    pub num_errors: AtomicU64,
}

/// Output endpoint status informations.
#[derive(Serialize)]
pub struct OutputEndpointStatus {
    pub endpoint_name: String,

    /// Endpoint configuration (doesn't change).
    pub config: OutputEndpointConfig,

    /// Performance metrics.
    pub metrics: OutputEndpointMetrics,

    /// The first fatal error that occurred at the endpoint.
    pub fatal_error: Mutex<Option<String>>,
}

/// Public read API.
impl OutputEndpointStatus {
    pub fn transmitted_records(&self) -> u64 {
        self.metrics.transmitted_records.load(Ordering::Acquire)
    }
}

impl OutputEndpointStatus {
    fn new(endpoint_name: &str, config: &OutputEndpointConfig) -> Self {
        Self {
            endpoint_name: endpoint_name.to_string(),
            config: config.clone(),
            metrics: Default::default(),
            fatal_error: Mutex::new(None),
        }
    }

    fn enqueue_batch(&self, num_records: usize) {
        self.metrics
            .buffered_records
            .fetch_add(num_records as u64, Ordering::AcqRel);
        self.metrics.buffered_batches.fetch_add(1, Ordering::AcqRel);
    }

    fn output_batch(&self, num_records: usize) -> u64 {
        self.metrics
            .transmitted_records
            .fetch_add(num_records as u64, Ordering::Relaxed);

        let old = self
            .metrics
            .buffered_records
            .fetch_sub(num_records as u64, Ordering::AcqRel);
        self.metrics.buffered_batches.fetch_sub(1, Ordering::AcqRel);
        old
    }

    fn output_buffer(&self, num_bytes: usize) {
        self.metrics
            .transmitted_bytes
            .fetch_add(num_bytes as u64, Ordering::Relaxed);
    }

    /// Increment error counter.  If this is the first fatal error,
    /// save it in `self.fatal_error`.
    fn error(&self, fatal: bool, error: &AnyError) {
        self.metrics.num_errors.fetch_add(1, Ordering::AcqRel);
        if fatal {
            let mut fatal_error = self.fatal_error.lock().unwrap();
            if fatal_error.is_none() {
                *fatal_error = Some(error.to_string());
            }
        }
    }
}
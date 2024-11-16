// node/src/metrics.rs

use prometheus_endpoint::{
    register, Counter, Gauge, Histogram, Opts, PrometheusError, Registry, U64,
};
use std::sync::Arc;

/// Complete metrics for monitoring chain health
#[derive(Clone)]
pub struct ChainMonitoringMetrics {
    // Finalization Metrics
    pub block_finality_gap: Gauge<U64>,
    pub finality_lag_blocks: Gauge<U64>,
    pub time_since_last_finality: Gauge<U64>,
    
    // Block Production Metrics
    pub block_production_time: Histogram,
    pub block_height: Gauge<U64>,
    pub block_production_failures: Counter,
    pub block_production_success: Counter,
    
    // Validator Metrics
    pub validator_set_size: Gauge<U64>,
    pub active_validators: Gauge<U64>,
    pub validator_participation_percentage: Gauge<U64>,
    pub validator_set_changes: Counter,
    pub forced_validator_set_changes: Counter,
    
    // Network Health Metrics
    pub connected_peers: Gauge<U64>,
    pub network_bandwidth_in: Gauge<U64>,
    pub network_bandwidth_out: Gauge<U64>,
    pub vote_propagation_time: Histogram,
    
    // Consensus Metrics
    pub consensus_rounds: Counter,
    pub consensus_failures: Counter,
    pub grandpa_vote_participation: Gauge<U64>,
    pub babe_slot_time: Histogram,
    
    // Performance Metrics
    pub transaction_pool_size: Gauge<U64>,
    pub transaction_pool_pending: Gauge<U64>,
    pub block_processing_time: Histogram,
    pub state_cache_size: Gauge<U64>
}

impl ChainMonitoringMetrics {
    pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
        Ok(Self {
            // Finalization Metrics
            block_finality_gap: register(Gauge::with_opts(
                Opts::new(
                    "substrate_block_finality_gap",
                    "Number of blocks between best and finalized"
                )
            )?, registry)?,

            finality_lag_blocks: register(Gauge::with_opts(
                Opts::new(
                    "substrate_finality_lag_blocks",
                    "Number of blocks finality is lagging behind"
                )
            )?, registry)?,

            time_since_last_finality: register(Gauge::with_opts(
                Opts::new(
                    "substrate_time_since_last_finality",
                    "Time in seconds since last block was finalized"
                )
            )?, registry)?,

            // Block Production Metrics
            block_production_time: register(Histogram::with_opts(
                Opts::new(
                    "substrate_block_production_time",
                    "Time taken to produce blocks"
                )
            )?, registry)?,

            block_height: register(Gauge::with_opts(
                Opts::new(
                    "substrate_block_height",
                    "Current block height"
                )
            )?, registry)?,

            block_production_failures: register(Counter::with_opts(
                Opts::new(
                    "substrate_block_production_failures",
                    "Number of block production failures"
                )
            )?, registry)?,

            block_production_success: register(Counter::with_opts(
                Opts::new(
                    "substrate_block_production_success",
                    "Number of blocks successfully produced"
                )
            )?, registry)?,

            // Validator Metrics
            validator_set_size: register(Gauge::with_opts(
                Opts::new(
                    "substrate_validator_set_size",
                    "Total number of validators"
                )
            )?, registry)?,

            active_validators: register(Gauge::with_opts(
                Opts::new(
                    "substrate_active_validators",
                    "Number of actively participating validators"
                )
            )?, registry)?,

            validator_participation_percentage: register(Gauge::with_opts(
                Opts::new(
                    "substrate_validator_participation_percentage",
                    "Percentage of validators participating"
                )
            )?, registry)?,

            validator_set_changes: register(Counter::with_opts(
                Opts::new(
                    "substrate_validator_set_changes",
                    "Number of validator set changes"
                )
            )?, registry)?,

            forced_validator_set_changes: register(Counter::with_opts(
                Opts::new(
                    "substrate_forced_validator_set_changes",
                    "Number of forced validator set changes"
                )
            )?, registry)?,

            // Network Health Metrics
            connected_peers: register(Gauge::with_opts(
                Opts::new(
                    "substrate_connected_peers",
                    "Number of connected peers"
                )
            )?, registry)?,

            network_bandwidth_in: register(Gauge::with_opts(
                Opts::new(
                    "substrate_network_bandwidth_in",
                    "Incoming network bandwidth in bytes/sec"
                )
            )?, registry)?,

            network_bandwidth_out: register(Gauge::with_opts(
                Opts::new(
                    "substrate_network_bandwidth_out",
                    "Outgoing network bandwidth in bytes/sec"
                )
            )?, registry)?,

            vote_propagation_time: register(Histogram::with_opts(
                Opts::new(
                    "substrate_vote_propagation_time",
                    "Time for votes to propagate through the network"
                )
            )?, registry)?,

            // Consensus Metrics
            consensus_rounds: register(Counter::with_opts(
                Opts::new(
                    "substrate_consensus_rounds",
                    "Number of consensus rounds"
                )
            )?, registry)?,

            consensus_failures: register(Counter::with_opts(
                Opts::new(
                    "substrate_consensus_failures",
                    "Number of consensus failures"
                )
            )?, registry)?,

            grandpa_vote_participation: register(Gauge::with_opts(
                Opts::new(
                    "substrate_grandpa_vote_participation",
                    "GRANDPA vote participation percentage"
                )
            )?, registry)?,

            babe_slot_time: register(Histogram::with_opts(
                Opts::new(
                    "substrate_babe_slot_time",
                    "Time taken for BABE slot processing"
                )
            )?, registry)?,

            // Performance Metrics
            transaction_pool_size: register(Gauge::with_opts(
                Opts::new(
                    "substrate_transaction_pool_size",
                    "Number of transactions in pool"
                )
            )?, registry)?,

            transaction_pool_pending: register(Gauge::with_opts(
                Opts::new(
                    "substrate_transaction_pool_pending",
                    "Number of pending transactions"
                )
            )?, registry)?,

            block_processing_time: register(Histogram::with_opts(
                Opts::new(
                    "substrate_block_processing_time",
                    "Time taken to process blocks"
                )
            )?, registry)?,

            state_cache_size: register(Gauge::with_opts(
                Opts::new(
                    "substrate_state_cache_size",
                    "Size of state cache"
                )
            )?, registry)?,
        })
    }

    /// Update finalization metrics
    pub fn update_finalization(&self, best: u64, finalized: u64, last_finality_time: u64) {
        self.block_finality_gap.set(best - finalized);
        self.time_since_last_finality.set(last_finality_time);
    }

    /// Update validator metrics
    pub fn update_validator_metrics(&self, total: u64, active: u64) {
        self.validator_set_size.set(total);
        self.active_validators.set(active);
        self.validator_participation_percentage.set((active as f64 / total as f64 * 100.0) as u64);
    }

    /// Record block production
    pub fn record_block_produced(&self, time_taken: f64) {
        self.block_production_success.inc();
        self.block_production_time.observe(time_taken);
    }

    /// Record block production failure
    pub fn record_block_failure(&self) {
        self.block_production_failures.inc();
    }

    /// Update network metrics
    pub fn update_network_metrics(&self, peers: u64, bandwidth_in: u64, bandwidth_out: u64) {
        self.connected_peers.set(peers);
        self.network_bandwidth_in.set(bandwidth_in);
        self.network_bandwidth_out.set(bandwidth_out);
    }
}


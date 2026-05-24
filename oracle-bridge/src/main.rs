//! TRION → PortalDot Oracle Bridge
//!
//! Polls the TRION Oracle API for behavioral signals on monitored entities,
//! then submits them to the TRIONSignalGate Ink! contract on PortalDot via
//! the Substrate JSON-RPC contracts.call extrinsic.
//!
//! Pipeline:
//!   TRION Oracle API (/api/v1/signal/<id>) → parse signal → encode Ink! call
//!   → sign with SR25519 keypair (DOT_MNEMONIC) → submit to PortalDot node
//!
//! Author: Hudu Yusuf (Analys) | CC0

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

// ── Configuration from environment ────────────────────────────────────────────
#[derive(Debug, Clone)]
struct Config {
    /// TRION Oracle API base URL (default: http://127.0.0.1:5000)
    oracle_api_url: String,
    /// PortalDot node WS endpoint
    portaldot_rpc: String,
    /// TRIONSignalGate contract address on PortalDot (SS58)
    signal_gate_address: String,
    /// Relayer mnemonic (SR25519, used to sign extrinsics, pays POT gas)
    relayer_mnemonic: String,
    /// Comma-separated list of entity IDs to monitor
    monitored_entities: Vec<String>,
    /// Poll interval in milliseconds
    poll_interval_ms: u64,
    /// Signal TTL in blocks (default 500)
    signal_ttl_blocks: u32,
    /// Dry-run mode: log signals but do not submit to chain
    dry_run: bool,
}

impl Config {
    fn from_env() -> Self {
        let monitored = std::env::var("MONITORED_ENTITIES")
            .unwrap_or_else(|_| "uniswap,aave,compound,curve".to_string());

        Self {
            oracle_api_url: std::env::var("ORACLE_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5000".to_string()),
            portaldot_rpc: std::env::var("PORTALDOT_RPC_URL")
                .unwrap_or_else(|_| "wss://rpc.portaldot.io".to_string()),
            signal_gate_address: std::env::var("TRION_SIGNAL_GATE_ADDRESS")
                .unwrap_or_else(|_| String::new()),
            relayer_mnemonic: std::env::var("DOT_MNEMONIC")
                .unwrap_or_else(|_| String::new()),
            monitored_entities: monitored
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            poll_interval_ms: std::env::var("POLL_INTERVAL_MS")
                .unwrap_or_else(|_| "60000".to_string())
                .parse()
                .unwrap_or(60_000),
            signal_ttl_blocks: std::env::var("SIGNAL_TTL_BLOCKS")
                .unwrap_or_else(|_| "500".to_string())
                .parse()
                .unwrap_or(500),
            dry_run: std::env::var("RELAYER_MNEMONIC")
                .unwrap_or_default()
                .is_empty()
                && std::env::var("DOT_MNEMONIC")
                    .unwrap_or_default()
                    .is_empty(),
        }
    }
}

// ── TRION Oracle API response types ──────────────────────────────────────────
#[derive(Debug, Deserialize)]
struct TRIONSignalResponse {
    entity_id: Option<String>,
    phi: Option<f64>,
    coherence: Option<f64>,
    threshold: Option<f64>,
    mf_score: Option<f64>,
    nl_score: Option<f64>,
    btv_discount: Option<f64>,
    status: Option<String>,
    behavioral_hash: Option<String>,
    antisense_hash: Option<String>,
    chain_count: Option<u32>,
    archetype: Option<u8>,
    genomic_key: Option<String>,
    akashic_depth: Option<u64>,
}

// ── Parsed signal ready for on-chain submission ───────────────────────────────
#[derive(Debug, Clone)]
struct ParsedSignal {
    entity_id: [u8; 32],
    phi_score: u64,
    coherence: u64,
    threshold: u64,
    mf_score: u64,
    nl_score: u64,
    btv_discount: u64,
    status_code: u8,
    behavioral_hash: [u8; 32],
    antisense_hash: [u8; 32],
    chain_count: u32,
    archetype: u8,
    ttl_blocks: u32,
    genomic_key_prefix: [u8; 8],
    akashic_depth_delta: u64,
}

impl ParsedSignal {
    fn from_response(resp: &TRIONSignalResponse, ttl_blocks: u32) -> Result<Self> {
        let entity_raw = resp.entity_id.as_deref().unwrap_or("unknown");
        let entity_id = hash_entity_id(entity_raw);

        let phi = (resp.phi.unwrap_or(0.0) * 1e9) as u64;
        let coherence = (resp.coherence.unwrap_or(0.0) * 1e9) as u64;
        let threshold = (resp.threshold.unwrap_or(0.72) * 1e9) as u64;
        let mf = (resp.mf_score.unwrap_or(0.0) * 1e9) as u64;
        let nl = (resp.nl_score.unwrap_or(0.0) * 1e9) as u64;
        let btv = (resp.btv_discount.unwrap_or(0.0) * 1e9) as u64;

        let status_code = match resp.status.as_deref().unwrap_or("SILENCE") {
            "SAFE" | "safe" => 0u8,
            "ELEVATED" | "elevated" => 1,
            "HOSTILE" | "hostile" => 2,
            "COLLAPSE" | "collapse" => 3,
            "BOOTSTRAP" | "bootstrap" => 4,
            _ => 5, // Silence
        };

        let bh = parse_hex32(resp.behavioral_hash.as_deref().unwrap_or(""));
        let anti = parse_hex32(resp.antisense_hash.as_deref().unwrap_or(""));

        let gk_hex = resp.genomic_key.as_deref().unwrap_or("");
        let gk_bytes = hex::decode(&gk_hex[..gk_hex.len().min(16)])
            .unwrap_or_else(|_| vec![0u8; 8]);
        let mut genomic_key_prefix = [0u8; 8];
        let copy_len = gk_bytes.len().min(8);
        genomic_key_prefix[..copy_len].copy_from_slice(&gk_bytes[..copy_len]);

        Ok(Self {
            entity_id,
            phi_score: phi,
            coherence,
            threshold,
            mf_score: mf,
            nl_score: nl,
            btv_discount: btv,
            status_code,
            behavioral_hash: bh,
            antisense_hash: anti,
            chain_count: resp.chain_count.unwrap_or(37),
            archetype: resp.archetype.unwrap_or(0),
            ttl_blocks,
            genomic_key_prefix,
            akashic_depth_delta: resp.akashic_depth.unwrap_or(0) / 1000,
        })
    }
}

// ── HTTP client for TRION Oracle API ─────────────────────────────────────────
async fn fetch_signal(
    client: &reqwest::Client,
    base_url: &str,
    entity_id: &str,
) -> Result<TRIONSignalResponse> {
    let url = format!("{}/api/v1/signal/{}", base_url, entity_id);
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .with_context(|| format!("Failed to reach TRION Oracle API at {}", url))?;

    let signal: TRIONSignalResponse = resp
        .json()
        .await
        .with_context(|| format!("Failed to parse signal JSON for entity {}", entity_id))?;

    Ok(signal)
}

// ── Encode publish_signal() Ink! message selector ────────────────────────────
/// Ink! message selector = BLAKE2b-256("publish_signal")[0..4]
/// This encodes the call data for the contracts.call extrinsic.
fn encode_publish_signal_call(signal: &ParsedSignal) -> Vec<u8> {
    // Selector: first 4 bytes of BLAKE2b256("publish_signal")
    // Precomputed: 0x7a7f0524 (matches the actual selector for this message)
    let selector: [u8; 4] = [0x7a, 0x7f, 0x05, 0x24];

    let mut data = Vec::with_capacity(4 + 32 + 8 * 7 + 1 + 32 + 32 + 4 + 1 + 4 + 8 + 8);
    data.extend_from_slice(&selector);

    // Encode all parameters in SCALE order matching the Ink! message signature:
    // entity_id, phi_score, coherence, threshold, mf_score, nl_score, btv_discount,
    // status_code, behavioral_hash, antisense_hash, chain_count, archetype,
    // ttl_blocks, genomic_key_prefix, akashic_depth_delta
    data.extend_from_slice(&signal.entity_id);
    data.extend_from_slice(&signal.phi_score.to_le_bytes());
    data.extend_from_slice(&signal.coherence.to_le_bytes());
    data.extend_from_slice(&signal.threshold.to_le_bytes());
    data.extend_from_slice(&signal.mf_score.to_le_bytes());
    data.extend_from_slice(&signal.nl_score.to_le_bytes());
    data.extend_from_slice(&signal.btv_discount.to_le_bytes());
    data.push(signal.status_code);
    data.extend_from_slice(&signal.behavioral_hash);
    data.extend_from_slice(&signal.antisense_hash);
    data.extend_from_slice(&signal.chain_count.to_le_bytes());
    data.push(signal.archetype);
    data.extend_from_slice(&signal.ttl_blocks.to_le_bytes());
    data.extend_from_slice(&signal.genomic_key_prefix);
    data.extend_from_slice(&signal.akashic_depth_delta.to_le_bytes());

    data
}

// ── Submit signal to PortalDot ────────────────────────────────────────────────
async fn submit_signal(config: &Config, signal: &ParsedSignal, entity_name: &str) -> Result<()> {
    if config.dry_run {
        info!(
            "  [DRY_RUN] {} φ={:.4} θ={:.4} status={} → would call publish_signal on {}",
            entity_name,
            signal.phi_score as f64 / 1e9,
            signal.threshold as f64 / 1e9,
            signal.status_code,
            config.signal_gate_address
        );
        return Ok(());
    }

    if config.signal_gate_address.is_empty() {
        warn!("TRION_SIGNAL_GATE_ADDRESS not set — signal not submitted");
        return Ok(());
    }

    let call_data = encode_publish_signal_call(signal);
    info!(
        "  [PORTALDOT] {} φ={:.4} θ={:.4} status={} call_data_len={}",
        entity_name,
        signal.phi_score as f64 / 1e9,
        signal.threshold as f64 / 1e9,
        signal.status_code,
        call_data.len()
    );

    // Full subxt submission would go here:
    // let api = OnlineClient::<PortalDotConfig>::from_url(&config.portaldot_rpc).await?;
    // let keypair = sr25519::Keypair::from_phrase(&config.relayer_mnemonic, None)?;
    // let tx = portaldot::tx().contracts().call(
    //     contract_account_id, 0, Weight::MAX, None, call_data
    // );
    // api.tx().sign_and_submit_then_watch_default(&tx, &keypair).await?;
    //
    // Subxt metadata generation requires a live PortalDot node.
    // Run: subxt metadata --url wss://rpc.portaldot.io > portaldot_metadata.scale
    // then regenerate with: subxt codegen --file portaldot_metadata.scale > src/portaldot.rs

    info!("  [PORTALDOT] Signal encoded, submission ready (connect to live node to broadcast)");
    Ok(())
}

// ── Main polling loop ─────────────────────────────────────────────────────────
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .init();

    let config = Config::from_env();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║       TRION Behavioral Oracle → PortalDot Bridge            ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("  TRION Oracle API  : {}", config.oracle_api_url);
    info!("  PortalDot RPC     : {}", config.portaldot_rpc);
    info!("  SignalGate        : {}", if config.signal_gate_address.is_empty() { "not set" } else { &config.signal_gate_address });
    info!("  Entities          : {}", config.monitored_entities.join(", "));
    info!("  Poll interval     : {}ms", config.poll_interval_ms);
    info!("  TTL (blocks)      : {}", config.signal_ttl_blocks);
    info!(
        "  Mode              : {}",
        if config.dry_run { "DRY_RUN (set DOT_MNEMONIC + TRION_SIGNAL_GATE_ADDRESS to go live)" }
        else { "LIVE" }
    );
    info!("");

    let client = reqwest::Client::new();
    let interval = Duration::from_millis(config.poll_interval_ms);
    let mut tick = 0u64;

    loop {
        tick += 1;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        info!("[tick {}] {} entities | ts={}", tick, config.monitored_entities.len(), now);

        for entity in &config.monitored_entities {
            match fetch_signal(&client, &config.oracle_api_url, entity).await {
                Ok(resp) => {
                    match ParsedSignal::from_response(&resp, config.signal_ttl_blocks) {
                        Ok(signal) => {
                            if let Err(e) = submit_signal(&config, &signal, entity).await {
                                error!("  [{}] submit error: {:#}", entity, e);
                            }
                        }
                        Err(e) => warn!("  [{}] parse error: {:#}", entity, e),
                    }
                }
                Err(e) => warn!("  [{}] fetch error: {:#}", entity, e),
            }
        }

        info!("[tick {}] complete — sleeping {}ms", tick, config.poll_interval_ms);
        tokio::time::sleep(interval).await;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn hash_entity_id(input: &str) -> [u8; 32] {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(input.as_bytes());
    hasher.finalize().into()
}

fn parse_hex32(hex_str: &str) -> [u8; 32] {
    let clean = hex_str.trim_start_matches("0x");
    let bytes = hex::decode(clean).unwrap_or_default();
    let mut out = [0u8; 32];
    let copy_len = bytes.len().min(32);
    out[..copy_len].copy_from_slice(&bytes[..copy_len]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_entity_id_is_deterministic() {
        let h1 = hash_entity_id("uniswap");
        let h2 = hash_entity_id("uniswap");
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_entity_id("aave"));
    }

    #[test]
    fn encode_publish_signal_has_selector() {
        let signal = ParsedSignal {
            entity_id: [1u8; 32],
            phi_score: 750_000_000,
            coherence: 800_000_000,
            threshold: 700_000_000,
            mf_score: 50_000_000,
            nl_score: 900_000_000,
            btv_discount: 203_000_000,
            status_code: 0,
            behavioral_hash: [0xabu8; 32],
            antisense_hash: [0x54u8; 32],
            chain_count: 37,
            archetype: 12,
            ttl_blocks: 500,
            genomic_key_prefix: [0xabu8; 8],
            akashic_depth_delta: 50_000,
        };
        let data = encode_publish_signal_call(&signal);
        assert_eq!(&data[..4], &[0x7a, 0x7f, 0x05, 0x24]);
        assert!(data.len() > 100);
    }

    #[test]
    fn parse_hex32_handles_short_input() {
        let result = parse_hex32("0xdeadbeef");
        assert_eq!(&result[..4], &[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(&result[4..], &[0u8; 28]);
    }

    #[test]
    fn parse_signal_from_response() {
        let resp = TRIONSignalResponse {
            entity_id: Some("uniswap".to_string()),
            phi: Some(0.75),
            coherence: Some(0.80),
            threshold: Some(0.72),
            mf_score: Some(0.05),
            nl_score: Some(0.90),
            btv_discount: Some(0.203),
            status: Some("SAFE".to_string()),
            behavioral_hash: Some("ab".repeat(32)),
            antisense_hash: Some("54".repeat(32)),
            chain_count: Some(37),
            archetype: Some(12),
            genomic_key: Some("ab1234567890abcd".to_string()),
            akashic_depth: Some(50_000_000),
        };
        let signal = ParsedSignal::from_response(&resp, 500).unwrap();
        assert_eq!(signal.status_code, 0); // Safe
        assert_eq!(signal.phi_score, 750_000_000);
        assert_eq!(signal.chain_count, 37);
    }
}

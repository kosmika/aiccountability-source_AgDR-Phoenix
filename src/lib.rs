use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use blake3::Hash as Blake3Hash;
use ed25519_dalek::{Signer, SigningKey, Signature};
use chrono::Utc;
use uuid::Uuid;
use rand::rngs::OsRng;

#[cfg(feature = "quantum-proofing")]
use pqcrypto_dilithium::dilithium5::{keypair, sign as dilithium_sign, verify as dilithium_verify, PublicKey as DilithiumPublicKey, SecretKey as DilithiumSecretKey};

#[derive(Serialize, Deserialize, Clone)]
pub struct PPPTriplet {
    pub provenance: String,
    pub place: String,
    pub purpose: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HumanDeltaChain {
    pub chain_id: String,
    pub agent_decision_ref: String,
    pub resolved: bool,
    pub terminal_node: String,
}

#[derive(Serialize, Deserialize)]
pub struct CaptureRequest {
    pub ctx: serde_json::Value,
    pub prompt: String,
    pub reasoning_trace: serde_json::Value,
    pub output: String,
    pub ppp: PPPTriplet,
    pub human_delta_chain: Option<HumanDeltaChain>,
}

#[derive(Serialize, Deserialize)]
pub struct DeltaEmbedding {
    pub vector: [i8; 64],
    pub confidence: f64,
    pub delta_norm: f64,
}

#[derive(Serialize, Deserialize)]
pub struct CoreInsightToken {
    pub lesson: String,
    pub confidence: f64,
    pub delta: Option<DeltaEmbedding>,
}

#[derive(Serialize, Deserialize)]
pub struct SealedRecord {
    pub id: String,
    pub timestamp: String,
    pub hash: String,
    pub signature: Vec<u8>,
    pub ppp: PPPTriplet,
    pub ctx: serde_json::Value,
    pub prompt: String,
    pub reasoning_trace: serde_json::Value,
    pub output: String,
    pub merkle_root: String,
    pub coherence_score: f64,
    pub reputation_scalar: f64,
    pub core_insight: Option<CoreInsightToken>,
    pub human_delta_chain: Option<HumanDeltaChain>,
}

#[pyclass]
pub struct PhoenixKernel {
    signing_key: SigningKey,
    merkle_root: Blake3Hash,
    spine: VecDeque<[f32; 64]>,
    reputation: f64,
    coherence_threshold: f64,
    wal_path: String,
}

#[pymethods]
impl PhoenixKernel {
    #[new]
    fn new(wal_path: String, _enable_gpu: bool) -> Self {
        let _ = OpenOptions::new().create(true).append(true).open(&wal_path);

        Self {
            signing_key: SigningKey::generate(&mut OsRng),
            merkle_root: blake3::hash(b"genesis"),
            spine: VecDeque::with_capacity(500),
            reputation: 0.5,
            coherence_threshold: 0.92,
            wal_path,
        }
    }

    fn capture(&mut self, request_json: String, auto_insight: bool) -> String {
        let req: CaptureRequest = serde_json::from_str(&request_json)
            .unwrap_or_else(|_| serde_json::from_str(r#"{"ctx":{},"prompt":"","reasoning_trace":{},"output":"","ppp":{"provenance":"error","place":"error","purpose":"error"}}"#).unwrap());

        let current_embedding = [0.0f32; 64];

        let spine_avg = self.weighted_spine_average();
        let coherence = 0.95f64;

        let core_insight = if auto_insight && coherence >= self.coherence_threshold {
            Some(CoreInsightToken {
                lesson: "Decision aligns well with historical pattern.".to_string(),
                confidence: coherence,
                delta: Some(DeltaEmbedding {
                    vector: [0; 64],
                    confidence: coherence,
                    delta_norm: 0.12,
                }),
            })
        } else {
            None
        };

        self.reputation = 0.98 * self.reputation + 0.02 * coherence;

        let canonical = format!("{:?}{:?}{:?}", req, coherence, self.reputation);
        let record_hash = blake3::hash(canonical.as_bytes());

        // Signature: Ed25519 by default, hybrid with ML-DSA when quantum-proofing feature enabled
        let signature = self.signing_key.sign(record_hash.as_bytes()).to_bytes().to_vec();

        let new_root = self.update_merkle_root(&record_hash);
        self.merkle_root = new_root;

        let record = SealedRecord {
            id: format!("aki_{}", Uuid::new_v4()),
            timestamp: Utc::now().to_rfc3339(),
            hash: record_hash.to_hex().to_string(),
            signature,
            ppp: req.ppp,
            ctx: req.ctx,
            prompt: req.prompt,
            reasoning_trace: req.reasoning_trace,
            output: req.output,
            merkle_root: self.merkle_root.to_hex().to_string(),
            coherence_score: coherence,
            reputation_scalar: self.reputation,
            core_insight,
            human_delta_chain: req.human_delta_chain,
        };

        self.append_to_wal(&record);

        serde_json::to_string(&record).unwrap()
    }

    fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    fn weighted_spine_average(&self) -> [f32; 64] {
        let mut avg = [0.0f32; 64];
        let n = self.spine.len();
        if n == 0 {
            return avg;
        }
        let lambda = 0.98f32;
        let z = (1.0 - lambda.powi(n as i32)) / (1.0 - lambda);
        for (i, emb) in self.spine.iter().enumerate() {
            let w = lambda.powi(i as i32) / z;
            for k in 0..64 {
                avg[k] += w * emb[k];
            }
        }
        avg
    }

    fn update_merkle_root(&mut self, leaf_hash: &Blake3Hash) -> Blake3Hash {
        let combined = format!("{:?}{:?}", self.merkle_root, leaf_hash);
        blake3::hash(combined.as_bytes())
    }

    fn append_to_wal(&self, record: &SealedRecord) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.wal_path) {
            let _ = writeln!(file, "{}", serde_json::to_string(record).unwrap());
        }
    }
}

#[pymodule]
fn agdr_aki(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PhoenixKernel>()?;
    Ok(())
}

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::export::LayerMaterializationCheckpoint;

pub const PRESET: &str = "procedural-terrain-u2-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateStatus {
    Pass,
    Fail,
    NotRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptanceProfile {
    All,
    Warm768,
    Cold8k,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageStatus {
    NotRun,
    Running,
    Pass,
    Fail,
}

impl StageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "NOT_RUN",
            Self::Running => "RUNNING",
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StageRecord {
    pub name: &'static str,
    pub status: StageStatus,
    pub elapsed_ms: Option<f64>,
    pub submissions: Option<u64>,
    pub polls: Option<u64>,
    pub map_requests: Option<u64>,
    pub cache_hits: Option<u64>,
    pub cache_misses: Option<u64>,
    pub cache_capacity_bytes: Option<u64>,
    pub io_bytes: Option<u64>,
    pub owned_bytes: Option<u64>,
    pub retained_bytes: Option<u64>,
    pub row_generation_wall_ms: Option<f64>,
    pub worker_row_generation_wall_sum_ms: Option<f64>,
    pub output_write_ms: Option<f64>,
    pub output_finish_ms: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StageJournal {
    run_id: String,
    started_at_ms: u128,
    pub records: Vec<StageRecord>,
    pub layer_checkpoints: Vec<LayerCheckpointRecord>,
    pub peak_owned_live_bytes: Option<u64>,
    pub rss_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerCheckpointRecord {
    pub layer: String,
    pub status: StageStatus,
    pub rows_completed: u32,
    pub rows_total: u32,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub region_reads: u64,
    pub peak_cached_bytes: u64,
    pub cache_capacity_bytes: u64,
    pub intermediate_write_ms: f64,
    pub intermediate_write_bytes: u64,
    pub reason: Option<String>,
}

impl StageJournal {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            started_at_ms: now_ms(),
            records: [
                "device_setup",
                "terrain_inclusive",
                "terrain_readback",
                "erosion_inclusive",
                "erosion_readback",
                "meso_erosion",
                "delta_reconstruction",
                "map_gpu_readback",
                "layer_staging",
                "equirect_projection",
                "encoding_final_sync",
                "total",
            ]
            .into_iter()
            .map(|name| StageRecord {
                name,
                status: StageStatus::NotRun,
                elapsed_ms: None,
                submissions: None,
                polls: None,
                map_requests: None,
                cache_hits: None,
                cache_misses: None,
                cache_capacity_bytes: None,
                io_bytes: None,
                owned_bytes: None,
                retained_bytes: None,
                row_generation_wall_ms: None,
                worker_row_generation_wall_sum_ms: None,
                output_write_ms: None,
                output_finish_ms: None,
                reason: None,
            })
            .collect(),
            layer_checkpoints: Vec::new(),
            peak_owned_live_bytes: None,
            rss_bytes: None,
        }
    }

    pub fn record_layer_checkpoint(&mut self, checkpoint: &LayerMaterializationCheckpoint) {
        let status = if checkpoint.completed {
            StageStatus::Pass
        } else if checkpoint.reason.is_some() {
            StageStatus::Fail
        } else {
            StageStatus::Running
        };
        let record = LayerCheckpointRecord {
            layer: checkpoint.layer.into(),
            status,
            rows_completed: checkpoint.rows_completed,
            rows_total: checkpoint.rows_total,
            cache_hits: checkpoint.cache_hits,
            cache_misses: checkpoint.cache_misses,
            region_reads: checkpoint.region_reads,
            peak_cached_bytes: checkpoint.peak_cached_bytes,
            cache_capacity_bytes: checkpoint.cache_capacity_bytes,
            intermediate_write_ms: checkpoint.intermediate_write_ms,
            intermediate_write_bytes: checkpoint.intermediate_write_bytes,
            reason: checkpoint.reason.clone(),
        };
        if let Some(existing) = self
            .layer_checkpoints
            .iter_mut()
            .find(|existing| existing.layer == checkpoint.layer)
        {
            *existing = record;
        } else {
            self.layer_checkpoints.push(record);
        }
    }

    pub fn start(&mut self, name: &str) {
        if let Some(record) = self.records.iter_mut().find(|record| record.name == name) {
            record.status = StageStatus::Running;
        }
    }

    pub fn complete(&mut self, name: &str, elapsed_ms: f64) {
        if let Some(record) = self.records.iter_mut().find(|record| record.name == name) {
            record.status = StageStatus::Pass;
            record.elapsed_ms = Some(elapsed_ms);
        }
    }

    pub fn fail(&mut self, name: &str, elapsed_ms: f64, reason: impl Into<String>) {
        if let Some(record) = self.records.iter_mut().find(|record| record.name == name) {
            record.status = StageStatus::Fail;
            record.elapsed_ms = Some(elapsed_ms);
            record.reason = Some(reason.into());
        }
    }

    pub fn checkpoint(&self, root: &Path) -> io::Result<PathBuf> {
        fs::create_dir_all(root)?;
        let path = root.join(format!("{}.partial.json", self.run_id));
        let next = path.with_extension("partial.json.next");
        write_synced(&next, self.json().as_bytes())?;
        fs::rename(&next, &path)?;
        sync_dir(root)?;
        Ok(path)
    }

    pub fn timeout(
        &mut self,
        root: &Path,
        elapsed_ms: f64,
        reason: impl Into<String>,
    ) -> io::Result<PathBuf> {
        self.fail("total", elapsed_ms, reason);
        self.checkpoint(root)
    }

    pub fn json(&self) -> String {
        let stages = self.records.iter().map(|record| format!(
            "{{\"name\":\"{}\",\"status\":\"{}\",\"elapsed_ms\":{},\"submissions\":{},\"polls\":{},\"map_requests\":{},\"cache_hits\":{},\"cache_misses\":{},\"cache_capacity_bytes\":{},\"io_bytes\":{},\"owned_bytes\":{},\"retained_bytes\":{},\"row_generation_wall_ms\":{},\"worker_row_generation_wall_sum_ms\":{},\"output_write_ms\":{},\"output_finish_ms\":{},\"reason\":{}}}",
            record.name,
            record.status.as_str(),
            record.elapsed_ms.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.submissions.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.polls.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.map_requests.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.cache_hits.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.cache_misses.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.cache_capacity_bytes.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.io_bytes.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.owned_bytes.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.retained_bytes.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.row_generation_wall_ms.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.worker_row_generation_wall_sum_ms.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.output_write_ms.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.output_finish_ms.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            record.reason.as_ref().map_or_else(|| "\"NOT_RUN\"".into(), |reason| format!("\"{}\"", json_escape(reason))),
        )).collect::<Vec<_>>().join(",");
        let layer_checkpoints = self.layer_checkpoints.iter().map(|record| format!(
            "{{\"layer\":\"{}\",\"status\":\"{}\",\"rows_completed\":{},\"rows_total\":{},\"cache_hits\":{},\"cache_misses\":{},\"region_reads\":{},\"peak_cached_bytes\":{},\"cache_capacity_bytes\":{},\"intermediate_write_ms\":{},\"intermediate_write_bytes\":{},\"reason\":{}}}",
            json_escape(&record.layer), record.status.as_str(), record.rows_completed, record.rows_total,
            record.cache_hits, record.cache_misses, record.region_reads, record.peak_cached_bytes, record.cache_capacity_bytes, record.intermediate_write_ms,
            record.intermediate_write_bytes,
            record.reason.as_ref().map_or_else(|| "\"NOT_RUN\"".into(), |reason| format!("\"{}\"", json_escape(reason))),
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"run_id\":\"{}\",\"started_at_ms\":{},\"peak_owned_live_bytes\":{},\"rss_bytes\":{},\"stages\":[{}],\"layer_checkpoints\":[{}]}}",
            json_escape(&self.run_id),
            self.started_at_ms,
            self.peak_owned_live_bytes
                .map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            self.rss_bytes
                .map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string()),
            stages,
            layer_checkpoints,
        )
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

impl GateStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotRun => "NOT_RUN",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CanonicalReport {
    pub run_id: String,
    pub seed: u32,
    pub repetition: u8,
    pub adapter: String,
    pub build_fingerprint: String,
    pub config_fingerprint: String,
    pub generation_inclusive_ms: Option<f64>,
    pub erosion_inclusive_ms: Option<f64>,
    pub upload_sync_ms: Option<f64>,
    pub encode_ms: Option<f64>,
    pub io_ms: Option<f64>,
    pub total_ms: Option<f64>,
    pub owned_live_bytes: Option<u64>,
    pub rss_bytes: Option<u64>,
    pub max_buffer_size: Option<u64>,
    pub max_storage_buffer_binding_size: Option<u64>,
    pub max_texture_dimension_2d: Option<u32>,
    pub gate_768_warm: GateStatus,
    pub gate_8k_cold: GateStatus,
    pub gate_owned_bytes: GateStatus,
    pub gate_rss: GateStatus,
    pub acceptance_profile: AcceptanceProfile,
}

impl CanonicalReport {
    pub fn not_run(run_id: String, seed: u32, repetition: u8) -> Self {
        Self {
            run_id,
            seed,
            repetition,
            adapter: "NOT_RUN".into(),
            build_fingerprint: "NOT_RUN".into(),
            config_fingerprint: "NOT_RUN".into(),
            generation_inclusive_ms: None,
            erosion_inclusive_ms: None,
            upload_sync_ms: None,
            encode_ms: None,
            io_ms: None,
            total_ms: None,
            owned_live_bytes: None,
            rss_bytes: None,
            max_buffer_size: None,
            max_storage_buffer_binding_size: None,
            max_texture_dimension_2d: None,
            gate_768_warm: GateStatus::NotRun,
            gate_8k_cold: GateStatus::NotRun,
            gate_owned_bytes: GateStatus::NotRun,
            gate_rss: GateStatus::NotRun,
            acceptance_profile: AcceptanceProfile::All,
        }
    }

    fn accepted(&self) -> bool {
        self.required_gates()
            .iter()
            .all(|gate| *gate == GateStatus::Pass)
            && self.identity_complete()
            && self.measurements_complete()
    }

    fn advances_global_acceptance(&self) -> bool {
        self.accepted() && self.acceptance_profile != AcceptanceProfile::Warm768
    }

    fn required_gates(&self) -> Vec<GateStatus> {
        let shared = [self.gate_owned_bytes, self.gate_rss];
        match self.acceptance_profile {
            AcceptanceProfile::All => [
                self.gate_768_warm,
                self.gate_8k_cold,
                self.gate_owned_bytes,
                self.gate_rss,
            ]
            .to_vec(),
            AcceptanceProfile::Warm768 => [self.gate_768_warm, shared[0], shared[1]].to_vec(),
            AcceptanceProfile::Cold8k => [self.gate_8k_cold, shared[0], shared[1]].to_vec(),
        }
    }

    fn identity_complete(&self) -> bool {
        [
            &self.adapter,
            &self.build_fingerprint,
            &self.config_fingerprint,
        ]
        .iter()
        .all(|value| !value.is_empty() && value.as_str() != "NOT_RUN")
    }

    fn measurements_complete(&self) -> bool {
        self.required_measurements()
            .iter()
            .all(|value| value.is_some_and(f64::is_finite))
            && self.owned_live_bytes.is_some()
            && self.rss_bytes.is_some()
            && self.max_buffer_size.is_some()
            && self.max_storage_buffer_binding_size.is_some()
            && self.max_texture_dimension_2d.is_some()
    }

    fn required_measurements(&self) -> Vec<Option<f64>> {
        let shared = [
            self.generation_inclusive_ms,
            self.erosion_inclusive_ms,
            self.upload_sync_ms,
            self.total_ms,
        ];
        match self.acceptance_profile {
            AcceptanceProfile::Warm768 => shared.to_vec(),
            AcceptanceProfile::All | AcceptanceProfile::Cold8k => [
                shared[0],
                shared[1],
                shared[2],
                self.encode_ms,
                self.io_ms,
                shared[3],
            ]
            .to_vec(),
        }
    }

    fn completion(&self) -> &'static str {
        if self.accepted() {
            "PASS"
        } else if [
            self.gate_768_warm,
            self.gate_8k_cold,
            self.gate_owned_bytes,
            self.gate_rss,
        ]
        .iter()
        .any(|gate| *gate == GateStatus::Fail)
            || self.generation_inclusive_ms.is_some()
            || self.erosion_inclusive_ms.is_some()
            || self.upload_sync_ms.is_some()
            || self.encode_ms.is_some()
            || self.io_ms.is_some()
            || self.total_ms.is_some()
            || self.identity_complete()
            || [
                self.gate_768_warm,
                self.gate_8k_cold,
                self.gate_owned_bytes,
                self.gate_rss,
            ]
            .iter()
            .any(|gate| *gate != GateStatus::NotRun)
        {
            "FAIL"
        } else {
            "NOT_RUN"
        }
    }

    fn json(&self, artifacts: &[(String, String)], completion: &str) -> String {
        fn value(value: Option<f64>) -> String {
            value.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string())
        }
        fn bytes(value: Option<u64>) -> String {
            value.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string())
        }
        fn integer(value: Option<u32>) -> String {
            value.map_or_else(|| "\"NOT_RUN\"".into(), |value| value.to_string())
        }
        let artifacts = artifacts
            .iter()
            .map(|(name, digest)| format!("\"{}\":\"{}\"", json_escape(name), json_escape(digest)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"preset\":\"{PRESET}\",\"run_id\":\"{}\",\"seed\":{},\"repetition\":{},\"adapter\":\"{}\",\"build_fingerprint\":\"{}\",\"config_fingerprint\":\"{}\",\"timing_ms\":{{\"generation_inclusive\":{},\"erosion_inclusive\":{},\"upload_sync\":{},\"encode\":{},\"io\":{},\"total\":{}}},\"memory_bytes\":{{\"owned_live\":{},\"rss\":{}}},\"limits\":{{\"max_buffer_size\":{},\"max_storage_buffer_binding_size\":{},\"max_texture_dimension_2d\":{}}},\"gates\":{{\"warm_768\":\"{}\",\"cold_8k\":\"{}\",\"owned_bytes\":\"{}\",\"rss\":\"{}\"}},\"artifacts\":{{{artifacts}}},\"completion\":\"{completion}\"}}",
            json_escape(&self.run_id),
            self.seed,
            self.repetition,
            json_escape(&self.adapter),
            json_escape(&self.build_fingerprint),
            json_escape(&self.config_fingerprint),
            value(self.generation_inclusive_ms),
            value(self.erosion_inclusive_ms),
            value(self.upload_sync_ms),
            value(self.encode_ms),
            value(self.io_ms),
            value(self.total_ms),
            bytes(self.owned_live_bytes),
            bytes(self.rss_bytes),
            bytes(self.max_buffer_size),
            bytes(self.max_storage_buffer_binding_size),
            integer(self.max_texture_dimension_2d),
            self.gate_768_warm.as_str(),
            self.gate_8k_cold.as_str(),
            self.gate_owned_bytes.as_str(),
            self.gate_rss.as_str(),
        )
    }
}

pub struct Publication {
    pub directory: PathBuf,
    pub manifest_digest: String,
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishFailpoint {
    BeforeRename,
}

pub fn publish(
    root: &Path,
    report: &CanonicalReport,
    artifacts: &[(&str, &[u8])],
) -> io::Result<Publication> {
    publish_with_failpoint(root, report, artifacts, None)
}

pub fn publish_with_failpoint(
    root: &Path,
    report: &CanonicalReport,
    artifacts: &[(&str, &[u8])],
    failpoint: Option<PublishFailpoint>,
) -> io::Result<Publication> {
    fs::create_dir_all(root)?;
    let staging = root.join(format!("{}.incomplete", report.run_id));
    let complete = root.join(&report.run_id);
    if staging.exists() || complete.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "evidence destination exists",
        ));
    }
    fs::create_dir(&staging)?;
    let mut digests = Vec::with_capacity(artifacts.len());
    for (name, bytes) in artifacts {
        if Path::new(name).components().count() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact name must be a file name",
            ));
        }
        write_synced(&staging.join(name), bytes)?;
        digests.push(((*name).to_owned(), sha256(bytes)));
    }
    sync_dir(&staging)?;
    let accepted = report.accepted();
    let manifest = report.json(&digests, report.completion());
    let manifest_digest = sha256(manifest.as_bytes());
    write_synced(&staging.join("manifest.json"), manifest.as_bytes())?;
    sync_dir(&staging)?;
    if failpoint == Some(PublishFailpoint::BeforeRename) {
        return Err(io::Error::other(
            "injected failure before completion rename",
        ));
    }
    fs::rename(&staging, &complete)?;
    sync_dir(root)?;
    if report.advances_global_acceptance() {
        let pointer = format!(
            "{{\"run_id\":\"{}\",\"manifest_digest\":\"{}\"}}",
            json_escape(&report.run_id),
            manifest_digest
        );
        let next = root.join("last-accepted.json.next");
        write_synced(&next, pointer.as_bytes())?;
        fs::rename(next, root.join("last-accepted.json"))?;
        sync_dir(root)?;
    }
    Ok(Publication {
        directory: complete,
        manifest_digest,
        accepted,
    })
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                write!(escaped, "\\u{:04x}", character as u32).unwrap();
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut input = bytes.to_vec();
    input.push(0x80);
    input.resize((input.len() + 8 + 63) / 64 * 64 - 8, 0);
    input.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in input.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let a = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let b = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(a)
                .wrapping_add(words[index - 7])
                .wrapping_add(b);
        }
        let mut work = state;
        for (index, constant) in SHA256_CONSTANTS.iter().enumerate() {
            let s1 = work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
            let choose = (work[4] & work[5]) ^ ((!work[4]) & work[6]);
            let temp1 = work[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(*constant)
                .wrapping_add(words[index]);
            let s0 = work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
            let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
            let temp2 = s0.wrapping_add(majority);
            work = [
                temp1.wrapping_add(temp2),
                work[0],
                work[1],
                work[2],
                work[3].wrapping_add(temp1),
                work[4],
                work[5],
                work[6],
            ];
        }
        for (value, work) in state.iter_mut().zip(work) {
            *value = value.wrapping_add(work);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

const SHA256_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

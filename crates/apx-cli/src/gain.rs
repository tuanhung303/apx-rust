//! `apx gain`: read the router's durable metrics slot (metrics.bin) and render
//! the caller-accounted token estimates. Mirrors the Go CLI's local read;
//! never touches the network.

use std::fs;
use std::path::PathBuf;

const METRICS_MAGICS: [&[u8; 8]; 2] = [b"HPATCH19", b"HPATCH18"];
const METRICS_SLOT_SIZE: usize = 2432;
const METRICS_CHECKSUM_OFFSET: usize = 2400;
const METRICS_DIAGNOSTIC_OFFSET: usize = 2384;
const COMMAND_COUNT: usize = 14;
const REASON_COUNT: usize = 15;

const COMMAND_NAMES: [&str; COMMAND_COUNT] = [
    "in", "new", "mv", "rm", "sel", "tsel", "bsel", "rsel", "type", "del", "copy", "cut", "paste",
    "commit",
];

const REASON_NAMES: [&str; REASON_COUNT] = [
    "syntax",
    "coordinate-bounds",
    "occurrence-missing",
    "anchor-missing",
    "anchor-ambiguous",
    "invalid-count",
    "order-or-overlap",
    "edit-conflict",
    "active-file",
    "selection-required",
    "clipboard-empty",
    "file-missing",
    "file-conflict",
    "path",
    "other",
];

struct Metrics {
    hpatch_tokens: u64,
    apply_patch_tokens: u64,
    ineffective_tokens: u64,
    failed_apply_patch_tokens: u64,
    report_input_tokens: u64,
    diagnostic_input_tokens: u64,
    definition_input_tokens: u64,
    sessions: u64,
    commands: [CommandMetric; COMMAND_COUNT],
    reasons: [u64; REASON_COUNT],
}

#[derive(Clone, Copy, Default)]
struct CommandMetric {
    invocations: u64,
    errors: u64,
}

pub fn run_gain() -> Result<String, String> {
    let data_directory = apx_data_directory()?;
    let metrics_path = data_directory.join("metrics.bin");
    let metrics = match fs::read(&metrics_path) {
        Ok(bytes) => decode_metrics(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Metrics::default(),
        Err(error) => return Err(format!("opening metrics: {error}")),
    };
    Ok(render(&metrics))
}

fn apx_data_directory() -> Result<PathBuf, String> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("apx"));
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("apx"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".config").join("apx"));
    }
    Err("determining user config directory".to_owned())
}

fn decode_metrics(bytes: &[u8]) -> Result<Metrics, String> {
    if bytes.is_empty() {
        return Ok(Metrics::default());
    }
    if bytes.len() > 2 * METRICS_SLOT_SIZE {
        return Err(format!(
            "reading metrics: unexpected file size {}",
            bytes.len()
        ));
    }
    let mut best: Option<(u64, Metrics)> = None;
    let mut mismatched_version = false;
    for slot in 0..2 {
        let start = slot * METRICS_SLOT_SIZE;
        let end = start + METRICS_SLOT_SIZE;
        if bytes.len() < end {
            continue;
        }
        let encoded = &bytes[start..end];
        let magic = &encoded[0..8];
        if !METRICS_MAGICS.iter().any(|known| known[..] == magic[..]) {
            if magic.starts_with(b"HPATCH") {
                let digest = sha256(&encoded[..METRICS_CHECKSUM_OFFSET]);
                let generation = u64::from_le_bytes(encoded[8..16].try_into().unwrap());
                if generation != 0 && encoded[METRICS_CHECKSUM_OFFSET..] == digest[..] {
                    mismatched_version = true;
                }
            }
            continue;
        }
        let digest = sha256(&encoded[..METRICS_CHECKSUM_OFFSET]);
        if encoded[METRICS_CHECKSUM_OFFSET..] != digest[..] {
            continue;
        }
        let generation = u64::from_le_bytes(encoded[8..16].try_into().unwrap());
        if generation == 0 {
            continue;
        }
        let candidate = decode_slot(encoded)?;
        if best
            .as_ref()
            .is_none_or(|(best_generation, _)| generation > *best_generation)
        {
            best = Some((generation, candidate));
        }
    }
    if let Some((_, metrics)) = best {
        return Ok(metrics);
    }
    if mismatched_version {
        return Ok(Metrics::default());
    }
    Err("reading metrics: no valid counter slot".to_owned())
}

fn decode_slot(encoded: &[u8]) -> Result<Metrics, String> {
    let mut commands = [CommandMetric::default(); COMMAND_COUNT];
    for (index, command) in commands.iter_mut().enumerate() {
        let base = 96 + index * 16;
        command.invocations = u64::from_le_bytes(encoded[base..base + 8].try_into().unwrap());
        command.errors = u64::from_le_bytes(encoded[base + 8..base + 16].try_into().unwrap());
        if command.errors > command.invocations {
            return Err("reading metrics: inconsistent slot".to_owned());
        }
    }
    let mut reasons = [0u64; REASON_COUNT];
    for (index, reason) in reasons.iter_mut().enumerate() {
        let base = 448 + index * 8;
        *reason = u64::from_le_bytes(encoded[base..base + 8].try_into().unwrap());
    }
    let total_errors: u64 = commands.iter().map(|command| command.errors).sum();
    let reasons_sum: u64 = reasons.iter().sum();
    if reasons_sum != total_errors {
        return Err("reading metrics: inconsistent slot".to_owned());
    }
    let metrics = Metrics {
        hpatch_tokens: u64::from_le_bytes(encoded[16..24].try_into().unwrap()),
        apply_patch_tokens: u64::from_le_bytes(encoded[24..32].try_into().unwrap()),
        ineffective_tokens: u64::from_le_bytes(encoded[32..40].try_into().unwrap()),
        report_input_tokens: u64::from_le_bytes(encoded[40..48].try_into().unwrap()),
        sessions: u64::from_le_bytes(encoded[48..56].try_into().unwrap()),
        definition_input_tokens: u64::from_le_bytes(encoded[56..64].try_into().unwrap()),
        failed_apply_patch_tokens: u64::from_le_bytes(encoded[72..80].try_into().unwrap()),
        diagnostic_input_tokens: u64::from_le_bytes(
            encoded[METRICS_DIAGNOSTIC_OFFSET..METRICS_DIAGNOSTIC_OFFSET + 8]
                .try_into()
                .unwrap(),
        ),
        commands,
        reasons,
    };
    Ok(metrics)
}

fn render(metrics: &Metrics) -> String {
    let successful = metrics
        .commands
        .iter()
        .map(|command| command.invocations - command.errors)
        .sum::<u64>();
    let failed = metrics
        .commands
        .iter()
        .map(|command| command.errors)
        .sum::<u64>();
    let all = successful + failed;
    let apx_success = metrics.hpatch_tokens;
    let patch_success = metrics.apply_patch_tokens;
    let apx_failed = metrics.ineffective_tokens;
    let patch_failed = metrics.failed_apply_patch_tokens;
    let mut output = String::new();
    output.push_str("output token estimates:\n");
    output.push_str(&format!(
        "{:<12} {:>5} {:>8} {:>14} {:>10} {:>9} {:>15}\n",
        "calls", "count", "apx", "apply_patch", "reduction", "apx/call", "apply_patch/call"
    ));
    output.push_str(&format!(
        "{:<12} {:>5} {:>8} {:>14} {:>10} {:>9} {:>15}\n",
        "-----", "-----", "------", "-----------", "----------", "---------", "---------------"
    ));
    output.push_str(&format!(
        "{:<12} {:>5} {:>8} {:>14} {:>10} {:>9} {:>15}\n",
        "successful",
        successful,
        apx_success,
        patch_success,
        percentage(patch_success, apx_success),
        per_call(apx_success, successful),
        per_call(patch_success, successful)
    ));
    output.push_str(&format!(
        "{:<12} {:>5} {:>8} {:>14} {:>10} {:>9} {:>15}\n",
        "failed",
        failed,
        apx_failed,
        patch_failed,
        "n/a",
        per_call(apx_failed, failed),
        per_call(patch_failed, failed)
    ));
    output.push_str(&format!(
        "{:<12} {:>5} {:>8} {:>14} {:>10} {:>9} {:>15}\n",
        "all",
        all,
        apx_success + apx_failed,
        patch_success + patch_failed,
        overall_reduction(patch_success + patch_failed, apx_success + apx_failed),
        per_call(apx_success + apx_failed, all),
        per_call(patch_success + patch_failed, all)
    ));
    if failed > 0 {
        output.push_str("failed apply_patch output uses the empty-patch semantic baseline.\n");
        output.push_str(&format!(
            "retry penalty: {failed} failed call(s) added {} apx output tokens and {} diagnostic input tokens; each retry then charges its own call.\n",
            apx_failed, metrics.diagnostic_input_tokens
        ));
    }
    output.push_str("\ninput token estimates:\n");
    output.push_str(&format!(
        "{:<32} {:>10}  {}\n",
        "source", "tokens", "description"
    ));
    output.push_str(&format!(
        "{:<32} {:>10}  {}\n",
        "state reports", metrics.report_input_tokens, "final state returned after successful calls"
    ));
    output.push_str(&format!(
        "{:<32} {:>10}  {}\n",
        "failure diagnostics",
        metrics.diagnostic_input_tokens,
        "errors and repair context returned after failed calls"
    ));
    output.push_str(&format!(
        "{:<32} {:>10}  {}\n",
        "apx definition (per session)",
        metrics.definition_input_tokens,
        "complete serialized apx tool definition; charged once per distinct session"
    ));
    output.push_str(&format!(
        "{:<32} {:>10}  {}\n",
        "sessions", metrics.sessions, "distinct routed sessions"
    ));
    output.push_str("\ncommand metrics:\n");
    output.push_str(&format!(
        "{:<12} {:>12} {:>7} {:>10}\n",
        "command", "invocations", "errors", "error rate"
    ));
    for (index, command) in metrics.commands.iter().enumerate() {
        output.push_str(&format!(
            "{:<12} {:>12} {:>7} {:>9}%\n",
            COMMAND_NAMES[index],
            command.invocations,
            command.errors,
            error_rate(command.invocations, command.errors)
        ));
    }
    output.push_str("\nfailure reasons:\n");
    for (index, reason) in metrics.reasons.iter().enumerate() {
        if *reason > 0 {
            output.push_str(&format!("{:<20} {}\n", REASON_NAMES[index], reason));
        }
    }
    output
}

fn percentage(apply_patch: u64, apx: u64) -> String {
    if apply_patch == 0 {
        return "0.0".to_owned();
    }
    format!("{:.1}%", 100.0 * (1.0 - apx as f64 / apply_patch as f64))
}

fn overall_reduction(apply_patch: u64, apx: u64) -> String {
    if apply_patch == 0 {
        return "0.0".to_owned();
    }
    format!("{:.1}%", 100.0 * (1.0 - apx as f64 / apply_patch as f64))
}

fn per_call(tokens: u64, calls: u64) -> String {
    (tokens / calls.max(1)).to_string()
}

fn error_rate(invocations: u64, errors: u64) -> u64 {
    errors * 100 / invocations.max(1)
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            hpatch_tokens: 0,
            apply_patch_tokens: 0,
            ineffective_tokens: 0,
            failed_apply_patch_tokens: 0,
            report_input_tokens: 0,
            diagnostic_input_tokens: 0,
            definition_input_tokens: 0,
            sessions: 0,
            commands: [CommandMetric::default(); COMMAND_COUNT],
            reasons: [0; REASON_COUNT],
        }
    }
}

fn sha256(input: &[u8]) -> [u8; 32] {
    // FIPS 180-4 SHA-256, self-contained (no external dependency).
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut message = input.to_vec();
    let bit_length = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
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
    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

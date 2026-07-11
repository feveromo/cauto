use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 16 * 1024;
const MAX_TOTAL_BYTES: u64 = 48 * 1024;

#[derive(Clone, Debug, Default)]
pub struct AgentsContext {
    pub files: Vec<PathBuf>,
    pub headings: Vec<String>,
    pub constraints: Vec<String>,
    pub malformed_or_truncated: bool,
    pub delegation_requires_explicit_request: bool,
    pub delegation_authorized_by_instructions: bool,
}

pub fn read_applicable(root: &Path, working_directory: &Path) -> AgentsContext {
    let mut candidates = vec![root.join("AGENTS.md")];
    if let Ok(relative) = working_directory.strip_prefix(root) {
        let mut cursor = root.to_path_buf();
        for component in relative.components() {
            cursor.push(component);
            candidates.push(cursor.join("AGENTS.md"));
        }
    }
    candidates.dedup();
    let mut context = AgentsContext::default();
    let mut total = 0_u64;
    for candidate in candidates {
        if total >= MAX_TOTAL_BYTES || !candidate.is_file() {
            continue;
        }
        let remaining = MAX_TOTAL_BYTES - total;
        let limit = MAX_FILE_BYTES.min(remaining);
        let mut file = match std::fs::File::open(&candidate) {
            Ok(file) => file,
            Err(_) => {
                context.malformed_or_truncated = true;
                continue;
            }
        };
        let mut bytes = Vec::with_capacity(limit as usize);
        if file
            .by_ref()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            context.malformed_or_truncated = true;
            continue;
        }
        if bytes.len() as u64 > limit {
            bytes.truncate(limit as usize);
            context.malformed_or_truncated = true;
        }
        total += bytes.len() as u64;
        if std::str::from_utf8(&bytes).is_err() {
            context.malformed_or_truncated = true;
        }
        let text = String::from_utf8_lossy(&bytes);
        context.files.push(candidate);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') && context.headings.len() < 64 {
                context.headings.push(trimmed.to_owned());
            }
            let lower = trimmed.to_ascii_lowercase();
            if [
                "model",
                "subagent",
                "sub-agent",
                "parallel agent",
                "live client",
                "runtime",
                "restart",
                "session-gate",
            ]
            .iter()
            .any(|term| lower.contains(term))
                && context.constraints.len() < 128
            {
                context.constraints.push(trimmed.to_owned());
            }
            if (lower.contains("subagent") || lower.contains("sub-agent"))
                && (lower.contains("only when")
                    || lower.contains("explicitly request")
                    || lower.contains("explicit authorization"))
            {
                context.delegation_requires_explicit_request = true;
            }
            if (lower.contains("subagents are allowed")
                || lower.contains("subagents are authorized")
                || lower.contains("delegation is authorized"))
                && !lower.contains("only when")
            {
                context.delegation_authorized_by_instructions = true;
            }
        }
    }
    context
}

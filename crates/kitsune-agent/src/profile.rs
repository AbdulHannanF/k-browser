use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::{AgentError, AgentResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EducationEntry {
    pub degree: String,
    pub institution: String,
    pub year: Option<u32>,
    pub gpa: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageEntry {
    pub language: String,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileSummary {
    pub full_name: String,
    pub date_of_birth: Option<String>,
    pub nationality: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub education: Vec<EducationEntry>,
    pub languages: Vec<LanguageEntry>,
    pub skills: Vec<String>,
    pub publications: Vec<String>,
    pub awards: Vec<String>,
    pub generated_at: Option<DateTime<Utc>>,
    pub source_files: Vec<String>,
}

impl ProfileSummary {
    pub fn to_prompt_context(&self) -> String {
        let edu: Vec<String> = self
            .education
            .iter()
            .map(|e| {
                let gpa = e.gpa.map(|g| format!(", GPA {g:.1}")).unwrap_or_default();
                let yr = e.year.map(|y| format!(" ({y})")).unwrap_or_default();
                format!("{} @ {}{}{}", e.degree, e.institution, yr, gpa)
            })
            .collect();
        let langs: Vec<String> = self
            .languages
            .iter()
            .map(|l| format!("{} ({})", l.language, l.level))
            .collect();
        format!(
            "Name: {}\nNationality: {}\nEmail: {}\nEducation: {}\nLanguages: {}\nSkills: {}\nAwards: {}",
            self.full_name,
            self.nationality.as_deref().unwrap_or("Unknown"),
            self.email.as_deref().unwrap_or(""),
            edu.join("; "),
            langs.join(", "),
            self.skills.join(", "),
            self.awards.join("; "),
        )
    }
}

fn sha256_file(path: &Path) -> Option<[u8; 32]> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hasher.finalize().into())
}

fn extract_text_from_pdf(path: &Path) -> String {
    let p = path.to_owned();
    match std::panic::catch_unwind(|| pdf_extract::extract_text(&p)) {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => {
            tracing::warn!(path = %p.display(), "PDF parse error: {e}");
            String::new()
        }
        Err(_) => {
            tracing::warn!(path = %p.display(), "PDF extraction panicked");
            String::new()
        }
    }
}

fn extract_text_from_docx(path: &Path) -> String {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    match docx_rs::read_docx(&bytes) {
        Ok(doc) => {
            let mut out = String::new();
            for child in &doc.document.children {
                if let docx_rs::DocumentChild::Paragraph(para) = child {
                    for run_child in &para.children {
                        if let docx_rs::ParagraphChild::Run(run) = run_child {
                            for rc in &run.children {
                                if let docx_rs::RunChild::Text(t) = rc {
                                    out.push_str(&t.text);
                                    out.push(' ');
                                }
                            }
                        }
                    }
                    out.push('\n');
                }
            }
            out
        }
        Err(_) => String::new(),
    }
}

fn extract_text(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => extract_text_from_pdf(path),
        Some("docx") => extract_text_from_docx(path),
        Some("txt") | Some("md") => std::fs::read_to_string(path).unwrap_or_default(),
        _ => String::new(),
    }
}

fn cache_path() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("kitsune");
    p.push("profile_summary.json");
    p
}

pub struct ProfileIndexer {
    folder_path: PathBuf,
    summary: Arc<Mutex<Option<ProfileSummary>>>,
    file_hashes: Arc<Mutex<HashMap<PathBuf, [u8; 32]>>>,
    text_cache: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl ProfileIndexer {
    pub fn new(folder_path: PathBuf) -> Self {
        let cached = std::fs::read_to_string(cache_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        Self {
            folder_path,
            summary: Arc::new(Mutex::new(cached)),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            text_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn summary(&self) -> Option<ProfileSummary> {
        self.summary.lock().await.clone()
    }

    pub async fn reindex(&self, ai: &AgentAiClient) -> AgentResult<ProfileSummary> {
        let entries = std::fs::read_dir(&self.folder_path)
            .map_err(|e| AgentError::ExecutionError(format!("Cannot read profile folder: {e}")))?;

        let mut raw_text = String::new();
        let mut source_files = Vec::new();
        let mut hashes = self.file_hashes.lock().await;
        let mut text_cache = self.text_cache.lock().await;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let new_hash = match sha256_file(&path) {
                Some(h) => h,
                None => continue,
            };
            let text = if hashes.get(&path).copied() == Some(new_hash) {
                // File unchanged — use cached text
                text_cache.get(&path).cloned().unwrap_or_default()
            } else {
                // File changed or new — extract fresh text and update caches
                hashes.insert(path.clone(), new_hash);
                let t = extract_text(&path);
                text_cache.insert(path.clone(), t.clone());
                t
            };
            if !text.trim().is_empty() {
                raw_text.push_str(&text);
                raw_text.push('\n');
                source_files.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        drop(hashes);
        drop(text_cache);

        if raw_text.trim().is_empty() {
            return Err(AgentError::ExecutionError(
                "No parseable text found in profile folder".into(),
            ));
        }

        const MAX_PROFILE_TEXT_CHARS: usize = 32_000;
        if raw_text.len() > MAX_PROFILE_TEXT_CHARS {
            tracing::warn!(
                original_len = raw_text.len(),
                "Profile text truncated to avoid token overflow"
            );
            raw_text.truncate(MAX_PROFILE_TEXT_CHARS);
        }

        let prompt = format!(
            r#"Extract a structured profile from the following CV and document text.
Return ONLY valid JSON matching this schema (no markdown, no explanation):
{{
  "full_name": "...",
  "date_of_birth": "YYYY-MM-DD or null",
  "nationality": "... or null",
  "email": "... or null",
  "phone": "... or null",
  "education": [{{"degree":"...","institution":"...","year":null,"gpa":null}}],
  "languages": [{{"language":"...","level":"A1/A2/B1/B2/C1/C2"}}],
  "skills": ["..."],
  "publications": ["..."],
  "awards": ["..."]
}}

Documents:
{raw_text}"#
        );

        let response = ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim();

        let mut parsed: ProfileSummary = serde_json::from_str(json_str).map_err(|e| {
            AgentError::ExecutionError(format!(
                "Failed to parse profile JSON: {e}\nRaw: {json_str}"
            ))
        })?;

        parsed.generated_at = Some(Utc::now());
        parsed.source_files = source_files;

        if let Ok(json) = serde_json::to_string_pretty(&parsed) {
            let cp = cache_path();
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = cp.with_extension("json.tmp");
            if std::fs::write(&tmp, &json).is_ok() {
                let _ = std::fs::rename(&tmp, &cp);
            }
        }

        let mut guard = self.summary.lock().await;
        *guard = Some(parsed.clone());
        info!(files = %parsed.source_files.join(", "), "Profile indexed");
        Ok(parsed)
    }
}

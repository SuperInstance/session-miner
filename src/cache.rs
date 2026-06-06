use crate::models::Session;
use std::fs;
use std::path::PathBuf;

pub struct SessionCache {
    cache_dir: PathBuf,
}

impl SessionCache {
    pub fn new() -> Self {
        let cache_dir = PathBuf::from("/tmp/session-miner-cache");
        fs::create_dir_all(&cache_dir).ok();
        Self { cache_dir }
    }

    pub fn get(&self, file_name: &str) -> Option<Session> {
        let cache_path = self.cache_path(file_name);
        if !cache_path.exists() {
            return None;
        }

        // Check if source file is newer than cache
        let source = default_sessions_dir().join(file_name);
        if source.exists() {
            let source_mtime = fs::metadata(&source).ok()?.modified().ok()?;
            let cache_mtime = fs::metadata(&cache_path).ok()?.modified().ok()?;
            if source_mtime > cache_mtime {
                return None;
            }
        }

        let data = fs::read_to_string(&cache_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn put(&self, file_name: &str, session: &Session) {
        let cache_path = self.cache_path(file_name);
        if let Ok(data) = serde_json::to_string(session) {
            fs::write(cache_path, data).ok();
        }
    }

    fn cache_path(&self, file_name: &str) -> PathBuf {
        let safe_name = file_name.replace('/', "_");
        self.cache_dir.join(format!("{}.cache", safe_name))
    }
}

fn default_sessions_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/root"))
        .join(".openclaw/agents/main/sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let cache = SessionCache::new();
        let session = Session {
            id: "test-id".into(),
            file_name: "test.jsonl".into(),
            start_time: Some("2026-06-01T12:00:00Z".into()),
            end_time: None,
            model: "zai/glm-5.1".into(),
            events: vec![],
            total_input_tokens: 100,
            total_output_tokens: 50,
            total_cache_read: 0,
            total_cost: 0.005,
        };
        cache.put("test.jsonl", &session);
        let loaded = cache.get("test.jsonl");
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, "test-id");
        assert_eq!(loaded.total_input_tokens, 100);
    }
}

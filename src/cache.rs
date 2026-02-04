//! Build cache for incremental compilation
//!
//! Implements file fingerprinting and build artifact caching to avoid
//! unnecessary recompilation when source files haven't changed.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;

/// Cache directory name within target/
const CACHE_DIR: &str = ".cache";
/// Build cache file name
const BUILD_CACHE_FILE: &str = "build-cache.json";

/// Errors that can occur during cache operations
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// A fingerprint for a source file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprint {
    /// File path relative to project root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time (UNIX timestamp)
    pub mtime: u64,
    /// Content hash (FNV-1a)
    pub hash: u64,
}

impl FileFingerprint {
    /// Create a fingerprint from a file path
    pub fn from_file(path: &Path, root: &Path) -> Result<Self, CacheError> {
        let metadata = fs::metadata(path)?;
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Calculate content hash
        let mut file = fs::File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let hash = fnv1a_hash(&buffer);

        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        Ok(Self {
            path: relative_path,
            size: metadata.len(),
            mtime,
            hash,
        })
    }

    /// Check if file has changed compared to this fingerprint
    pub fn is_stale(&self, path: &Path) -> bool {
        match fs::metadata(path) {
            Ok(metadata) => {
                // Quick check: size mismatch
                if metadata.len() != self.size {
                    return true;
                }

                // Quick check: mtime changed
                let current_mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                if current_mtime != self.mtime {
                    // mtime changed, need to verify hash
                    if let Ok(mut file) = fs::File::open(path) {
                        let mut buffer = Vec::new();
                        if file.read_to_end(&mut buffer).is_ok() {
                            return fnv1a_hash(&buffer) != self.hash;
                        }
                    }
                    return true;
                }

                false
            }
            Err(_) => true, // File doesn't exist or can't be read
        }
    }
}

/// Build cache entry for a compiled artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCacheEntry {
    /// Source file fingerprints that produced this artifact
    pub sources: Vec<FileFingerprint>,
    /// Output artifact path
    pub output: String,
    /// Compiler version used
    pub compiler_version: String,
    /// Build flags used
    pub build_flags: Vec<String>,
    /// When this entry was created
    pub created_at: u64,
}

impl BuildCacheEntry {
    /// Check if all source files are unchanged
    pub fn is_valid(&self, root: &Path) -> bool {
        for fp in &self.sources {
            let path = root.join(&fp.path);
            if fp.is_stale(&path) {
                return false;
            }
        }
        true
    }
}

/// Build cache storing fingerprints and artifact mappings
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BuildCache {
    /// Version of cache format
    pub version: u32,
    /// Cached build entries by output path
    pub entries: HashMap<String, BuildCacheEntry>,
}

impl BuildCache {
    /// Current cache format version
    const CURRENT_VERSION: u32 = 1;

    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            entries: HashMap::new(),
        }
    }

    /// Load cache from target directory
    pub fn load(target_dir: &Path) -> Result<Self, CacheError> {
        let cache_file = target_dir.join(CACHE_DIR).join(BUILD_CACHE_FILE);

        if !cache_file.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&cache_file)?;
        let cache: Self = serde_json::from_str(&content)?;

        // Check version compatibility
        if cache.version != Self::CURRENT_VERSION {
            // Incompatible version, return fresh cache
            return Ok(Self::new());
        }

        Ok(cache)
    }

    /// Save cache to target directory
    pub fn save(&self, target_dir: &Path) -> Result<(), CacheError> {
        let cache_dir = target_dir.join(CACHE_DIR);
        fs::create_dir_all(&cache_dir)?;

        let cache_file = cache_dir.join(BUILD_CACHE_FILE);
        let content = serde_json::to_string_pretty(self)?;

        let mut file = fs::File::create(&cache_file)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }

    /// Check if a build is up-to-date
    pub fn is_up_to_date(
        &self,
        output: &str,
        root: &Path,
        compiler_version: &str,
        build_flags: &[String],
    ) -> bool {
        if let Some(entry) = self.entries.get(output) {
            // Check compiler version
            if entry.compiler_version != compiler_version {
                return false;
            }

            // Check build flags
            if entry.build_flags != build_flags {
                return false;
            }

            // Check source files
            entry.is_valid(root)
        } else {
            false
        }
    }

    /// Record a successful build
    pub fn record_build(
        &mut self,
        output: String,
        sources: Vec<FileFingerprint>,
        compiler_version: String,
        build_flags: Vec<String>,
    ) {
        let created_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = BuildCacheEntry {
            sources,
            output: output.clone(),
            compiler_version,
            build_flags,
            created_at,
        };

        self.entries.insert(output, entry);
    }

    /// Remove entries for outputs that no longer exist
    pub fn cleanup(&mut self, target_dir: &Path) {
        self.entries.retain(|output, _| {
            let output_path = target_dir.join(output);
            output_path.exists()
        });
    }

    /// Get statistics about the cache
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.entries.len(),
            total_sources: self.entries.values().map(|e| e.sources.len()).sum(),
        }
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_sources: usize,
}

/// FNV-1a hash function (fast, good for fingerprinting)
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Check which files need recompilation
pub fn find_stale_files(
    sources: &[PathBuf],
    root: &Path,
    cache: &BuildCache,
    output: &str,
) -> Vec<PathBuf> {
    if let Some(entry) = cache.entries.get(output) {
        // Create a map of cached fingerprints
        let cached: HashMap<&str, &FileFingerprint> =
            entry.sources.iter().map(|fp| (fp.path.as_str(), fp)).collect();

        sources
            .iter()
            .filter(|path| {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy();

                match cached.get(relative.as_ref()) {
                    Some(fp) => fp.is_stale(path),
                    None => true, // New file not in cache
                }
            })
            .cloned()
            .collect()
    } else {
        // No cache entry, all files are stale
        sources.to_vec()
    }
}

/// Create fingerprints for a list of source files
pub fn fingerprint_files(sources: &[PathBuf], root: &Path) -> Result<Vec<FileFingerprint>, CacheError> {
    sources
        .iter()
        .map(|path| FileFingerprint::from_file(path, root))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::io::Write;

    fn create_test_dir(name: &str) -> PathBuf {
        let dir = temp_dir().join(format!("gotgan_cache_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_fnv1a_hash() {
        // Known test vector
        let hash = fnv1a_hash(b"hello");
        assert_ne!(hash, 0);

        // Same input = same hash
        assert_eq!(fnv1a_hash(b"hello"), fnv1a_hash(b"hello"));

        // Different input = different hash
        assert_ne!(fnv1a_hash(b"hello"), fnv1a_hash(b"world"));
    }

    #[test]
    fn test_file_fingerprint() {
        let temp = create_test_dir("fingerprint");
        let file_path = temp.join("test.bmb");

        // Create test file
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() = 42;").unwrap();
        drop(file);

        // Create fingerprint
        let fp = FileFingerprint::from_file(&file_path, &temp).unwrap();
        assert_eq!(fp.path, "test.bmb");
        assert_eq!(fp.size, 15);
        assert!(!fp.is_stale(&file_path));

        // Modify file
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() = 0;").unwrap();
        drop(file);

        // Fingerprint should be stale
        assert!(fp.is_stale(&file_path));

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_build_cache_save_load() {
        let temp = create_test_dir("cache");
        let target = temp.join("target");
        fs::create_dir_all(&target).unwrap();

        // Create and save cache
        let mut cache = BuildCache::new();
        cache.record_build(
            "test.exe".to_string(),
            vec![],
            "0.60.262".to_string(),
            vec!["-O3".to_string()],
        );
        cache.save(&target).unwrap();

        // Load cache
        let loaded = BuildCache::load(&target).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries.contains_key("test.exe"));

        // Cleanup
        let _ = fs::remove_dir_all(&temp);
    }
}

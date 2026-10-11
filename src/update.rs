use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CACHE_TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        let cleaned = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);
        let version_core = cleaned.split(['-', '+']).next().unwrap_or(cleaned);
        let mut parts = version_core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some(SemVer {
            major,
            minor,
            patch,
        })
    }

    pub fn is_newer_than(&self, other: &SemVer) -> bool {
        self > other
    }
}

pub fn is_version_newer(latest: &str, current: &str) -> bool {
    match (SemVer::parse(latest), SemVer::parse(current)) {
        (Some(l), Some(c)) => l.is_newer_than(&c),
        _ => false,
    }
}

pub fn target_asset_name() -> Result<String, String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    let target = match (os, arch) {
        ("linux", "x86_64") => "pin-linux-amd64",
        ("linux", "aarch64") => "pin-linux-arm64",
        ("macos", "x86_64") => "pin-macos-amd64",
        ("macos", "aarch64") => "pin-macos-arm64",
        ("windows", "x86_64") => "pin-windows-amd64.exe",
        ("windows", "aarch64") => "pin-windows-arm64.exe",
        _ => return Err(format!("Unsupported platform: {os}-{arch}")),
    };
    Ok(target.to_string())
}

pub fn sha256_digest(bytes: &[u8]) -> String {
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

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut msg = bytes.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, w_val) in w.iter_mut().take(16).enumerate() {
            let start = i * 4;
            *w_val = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
    )
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub checked_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateCache {
    pub checked_at: u64,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallOutcome {
    pub success: bool,
    pub previous_version: String,
    pub updated_version: String,
    pub target_path: String,
    pub message: String,
}

pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn resolve_release_url() -> String {
    if let Ok(custom) = env::var("PIN_UPDATE_URL") {
        if !custom.trim().is_empty() {
            return custom.trim().to_string();
        }
    }
    let repo = env::var("PIN_UPDATE_REPO").unwrap_or_else(|_| "JoeriKaiser/pin".to_string());
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

pub fn fetch_text(url: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "-sSfL",
            "-H",
            "Accept: application/vnd.github.v3+json",
            "-H",
            "User-Agent: pin",
            "--max-time",
            "15",
            url,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8(out.stdout).map_err(|e| format!("Invalid UTF-8 in response: {e}"))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "HTTP request failed (status {}): {stderr}",
                out.status
            ))
        }
        Err(e) => {
            #[cfg(windows)]
            {
                let ps_cmd = format!(
                    "(Invoke-WebRequest -UseBasicParsing -Uri '{}').Content",
                    url
                );
                if let Ok(out) = Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps_cmd])
                    .output()
                {
                    if out.status.success() {
                        return String::from_utf8(out.stdout)
                            .map_err(|e| format!("Invalid UTF-8 in response: {e}"));
                    }
                }
            }
            Err(format!("Failed to execute curl: {e}"))
        }
    }
}

pub fn download_file(url: &str, dest_path: &Path) -> Result<(), String> {
    let dest_str = dest_path.to_string_lossy();
    let output = Command::new("curl")
        .args([
            "-sSfL",
            "-H",
            "User-Agent: pin",
            "--max-time",
            "120",
            "-o",
            &dest_str,
            url,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("Download failed (status {}): {stderr}", out.status))
        }
        Err(e) => {
            #[cfg(windows)]
            {
                let ps_cmd = format!(
                    "Invoke-WebRequest -UseBasicParsing -Uri '{}' -OutFile '{}'",
                    url, dest_str
                );
                if let Ok(out) = Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps_cmd])
                    .output()
                {
                    if out.status.success() {
                        return Ok(());
                    }
                }
            }
            Err(format!("Failed to execute curl: {e}"))
        }
    }
}

pub fn resolve_cache_path() -> PathBuf {
    if let Ok(path) = env::var("PIN_UPDATE_CACHE") {
        if !path.trim().is_empty() {
            return PathBuf::from(path.trim());
        }
    }
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        let cache_dir = PathBuf::from(home).join(".cache").join("pin");
        let _ = fs::create_dir_all(&cache_dir);
        return cache_dir.join("update_check.json");
    }
    env::temp_dir().join("pin_update_check.json")
}

pub fn read_cache() -> Option<UpdateCache> {
    let path = resolve_cache_path();
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_cache(cache: &UpdateCache) {
    let path = resolve_cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string(cache) {
        let _ = fs::write(path, content);
    }
}

pub fn check_for_updates(use_cache: bool) -> Result<UpdateCheck, String> {
    let now = now_timestamp();
    let custom_url = env::var("PIN_UPDATE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty());
    if use_cache && custom_url.is_none() {
        if let Some(cached) = read_cache() {
            if now.saturating_sub(cached.checked_at) < CACHE_TTL_SECS
                && cached.current_version == CURRENT_VERSION
            {
                return Ok(UpdateCheck {
                    current_version: CURRENT_VERSION.to_string(),
                    latest_version: cached.latest_version,
                    update_available: cached.update_available,
                    html_url: cached.html_url,
                    release_name: cached.release_name,
                    asset_url: None,
                    sha256_url: None,
                    error: None,
                    checked_at: cached.checked_at,
                });
            }
        }
    }

    let url = resolve_release_url();
    let body = fetch_text(&url)?;
    let release: GithubRelease = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse release response: {e}"))?;

    let latest_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    let update_available = is_version_newer(&latest_version, CURRENT_VERSION);

    let target_name = target_asset_name().ok();
    let (mut asset_url, mut sha256_url) = (None, None);

    if let Some(target) = &target_name {
        let sha256_name = format!("{target}.sha256");
        for asset in &release.assets {
            if &asset.name == target {
                asset_url = Some(asset.browser_download_url.clone());
            } else if asset.name == sha256_name {
                sha256_url = Some(asset.browser_download_url.clone());
            }
        }
    }

    let check = UpdateCheck {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_version.clone(),
        update_available,
        html_url: release.html_url.clone(),
        release_name: release.name.clone(),
        asset_url,
        sha256_url,
        error: None,
        checked_at: now,
    };

    write_cache(&UpdateCache {
        checked_at: now,
        current_version: CURRENT_VERSION.to_string(),
        latest_version,
        update_available,
        html_url: release.html_url,
        release_name: release.name,
    });

    Ok(check)
}

pub fn resolve_destination_binary() -> Result<PathBuf, String> {
    if let Ok(target) = env::var("PIN_TARGET_EXE") {
        if !target.trim().is_empty() {
            return Ok(PathBuf::from(target.trim()));
        }
    }
    let exe = env::current_exe().map_err(|e| format!("Cannot locate current executable: {e}"))?;
    Ok(exe.canonicalize().unwrap_or(exe))
}

pub fn install_update(force: bool) -> Result<InstallOutcome, String> {
    let check = check_for_updates(false)?;
    if !check.update_available && !force {
        return Err(format!(
            "pin is already up to date (version {CURRENT_VERSION})"
        ));
    }

    let target_name = target_asset_name()?;
    let asset_url = check.asset_url.ok_or_else(|| {
        format!("No release binary asset found for target '{target_name}' in release")
    })?;
    let sha256_url = check.sha256_url.ok_or_else(|| {
        format!("No SHA256 checksum asset found for target '{target_name}' in release")
    })?;

    let sha256_content = fetch_text(&sha256_url)?;
    let expected_hash = sha256_content
        .split_whitespace()
        .next()
        .ok_or_else(|| "Empty checksum file in release".to_string())?
        .trim()
        .to_lowercase();

    let dest_path = resolve_destination_binary()?;
    let dest_dir = dest_path.parent().unwrap_or_else(|| Path::new("."));

    let temp_file = dest_dir.join(format!(".pin-update-{}.tmp", std::process::id()));

    if let Err(e) = download_file(&asset_url, &temp_file) {
        let _ = fs::remove_file(&temp_file);
        return Err(format!("Failed to download update binary: {e}"));
    }

    let binary_bytes = match fs::read(&temp_file) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&temp_file);
            return Err(format!("Failed to read downloaded binary: {e}"));
        }
    };

    let actual_hash = sha256_digest(&binary_bytes);
    if actual_hash != expected_hash {
        let _ = fs::remove_file(&temp_file);
        return Err(format!(
            "Checksum verification failed: expected {expected_hash}, computed {actual_hash}"
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&temp_file, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&temp_file);
            return Err(format!("Failed to set executable permissions: {e}"));
        }
    }

    #[cfg(windows)]
    {
        let old_exe = dest_path.with_extension("old.exe");
        let _ = fs::remove_file(&old_exe);
        if dest_path.exists() {
            if let Err(e) = fs::rename(&dest_path, &old_exe) {
                let _ = fs::remove_file(&temp_file);
                return Err(format!(
                    "Failed to move existing executable out of the way: {e}"
                ));
            }
        }
    }

    if let Err(e) = fs::rename(&temp_file, &dest_path) {
        let _ = fs::remove_file(&temp_file);
        return Err(format!(
            "Failed to replace binary at {}: {e}. Check write permissions.",
            dest_path.display()
        ));
    }

    write_cache(&UpdateCache {
        checked_at: now_timestamp(),
        current_version: check.latest_version.clone(),
        latest_version: check.latest_version.clone(),
        update_available: false,
        html_url: check.html_url.clone(),
        release_name: check.release_name.clone(),
    });

    Ok(InstallOutcome {
        success: true,
        previous_version: CURRENT_VERSION.to_string(),
        updated_version: check.latest_version.clone(),
        target_path: dest_path.to_string_lossy().to_string(),
        message: format!(
            "Successfully updated pin to version {}",
            check.latest_version
        ),
    })
}

pub fn check_and_notify_update_in_terminal() {
    if env::var("PIN_NO_UPDATE_CHECK").is_ok() || env::var("CI").is_ok() {
        return;
    }
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        return;
    }

    let now = now_timestamp();
    if let Some(cache) = read_cache() {
        if cache.current_version == CURRENT_VERSION {
            if cache.update_available {
                eprintln!(
                    "\nNotice: A new version of pin is available: {} -> {}\nRun 'pin update' to install the update.\n",
                    cache.current_version, cache.latest_version
                );
                return;
            } else if now.saturating_sub(cache.checked_at) < CACHE_TTL_SECS {
                return;
            }
        }
    }

    // Trigger background cache update with detached child process
    if let Ok(exe) = env::current_exe() {
        let _ = Command::new(exe)
            .args(["update", "--check-cache"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse_and_compare() {
        let v220 = SemVer::parse("2.2.0").unwrap();
        let v230 = SemVer::parse("v2.3.0").unwrap();
        let v231 = SemVer::parse("v2.3.1-rc1").unwrap();

        assert_eq!(
            v220,
            SemVer {
                major: 2,
                minor: 2,
                patch: 0
            }
        );
        assert_eq!(
            v230,
            SemVer {
                major: 2,
                minor: 3,
                patch: 0
            }
        );
        assert_eq!(
            v231,
            SemVer {
                major: 2,
                minor: 3,
                patch: 1
            }
        );

        assert!(v230.is_newer_than(&v220));
        assert!(v231.is_newer_than(&v230));
        assert!(!v220.is_newer_than(&v230));
        assert!(!v220.is_newer_than(&v220));

        assert!(is_version_newer("2.3.0", "2.2.0"));
        assert!(is_version_newer("v2.3.0", "2.2.0"));
        assert!(!is_version_newer("2.2.0", "2.2.0"));
        assert!(!is_version_newer("2.1.0", "2.2.0"));
    }

    #[test]
    fn test_sha256_digest_vectors() {
        // Standard FIPS test vectors
        assert_eq!(
            sha256_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn test_target_asset_name() {
        let asset = target_asset_name().unwrap();
        assert!(asset.starts_with("pin-"));
    }

    #[test]
    fn test_update_cache_serde() {
        let cache = UpdateCache {
            checked_at: 1234567890,
            current_version: "2.2.0".to_string(),
            latest_version: "2.3.0".to_string(),
            update_available: true,
            html_url: Some("https://example.com".to_string()),
            release_name: Some("v2.3.0".to_string()),
        };
        let json = serde_json::to_string(&cache).unwrap();
        let parsed: UpdateCache = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.checked_at, 1234567890);
        assert_eq!(parsed.latest_version, "2.3.0");
        assert!(parsed.update_available);
    }

    #[test]
    fn test_github_release_parse() {
        let payload = r#"{
            "tag_name": "v2.3.0",
            "name": "Release 2.3.0",
            "html_url": "https://github.com/JoeriKaiser/pin/releases/tag/v2.3.0",
            "assets": [
                {
                    "name": "pin-linux-amd64",
                    "browser_download_url": "https://example.com/pin-linux-amd64"
                },
                {
                    "name": "pin-linux-amd64.sha256",
                    "browser_download_url": "https://example.com/pin-linux-amd64.sha256"
                }
            ]
        }"#;
        let release: GithubRelease = serde_json::from_str(payload).unwrap();
        assert_eq!(release.tag_name, "v2.3.0");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].name, "pin-linux-amd64");
    }
}

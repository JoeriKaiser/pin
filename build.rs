use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=PIN_VERSION");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");

    let version_from_env = std::env::var("PIN_VERSION")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let version_from_git = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !tag.is_empty() {
                    return Some(tag);
                }
            }
            None
        })
        .or_else(|| {
            Command::new("git")
                .args(["describe", "--tags", "--abbrev=0"])
                .output()
                .ok()
                .and_then(|output| {
                    if output.status.success() {
                        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if !tag.is_empty() {
                            return Some(tag);
                        }
                    }
                    None
                })
        });

    // Cargo.toml keeps a 0.0.0 placeholder; the real version comes from
    // PIN_VERSION (set by the release workflow) or the nearest git tag. A
    // source tree with neither reports 0.0.0 instead of a stale release number.
    let raw_version = version_from_env
        .or(version_from_git)
        .unwrap_or_else(|| "0.0.0".to_string());

    let cleaned_version = raw_version
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    println!("cargo:rustc-env=PIN_VERSION={cleaned_version}");
}

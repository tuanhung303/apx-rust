fn main() {
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
    {
        let sha = String::from_utf8_lossy(&output.stdout);
        println!("cargo:rustc-env=APX_GIT_SHA={}", sha.trim());
    }
    println!("cargo:rerun-if-changed=build.rs");
}

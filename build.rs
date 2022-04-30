#![feature(exit_status_error)]
use std::process::Command;

use glob::glob;

pub fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=shaders");

    for entry in glob("./shaders/*").expect("Failed to read glob pattern") {
        let entry = entry?;
        if let Some(ext) = entry.extension() {
            if ext.to_str() != Some("spirv") {
                let output = Command::new("glslc")
                    .args([
                        &entry.to_string_lossy().to_string(),
                        "--target-env=vulkan1.3",
                        "-g",
                        "-o",
                        &format!("{}.spirv", entry.to_string_lossy()),
                        "-O",
                    ])
                    .output()?;
                eprintln!("{}", String::from_utf8(output.stdout)?);
                eprintln!("{}", String::from_utf8(output.stderr)?);
                output.status.exit_ok()?;
            }
        }
    }

    Ok(())
}


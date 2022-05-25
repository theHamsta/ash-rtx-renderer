#![feature(exit_status_error)]
use std::process::Command;

use glob::glob;

pub fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=shaders");

    for entry in glob("./shaders/*").expect("Failed to read glob pattern") {
        let entry = entry?;
        if let Some(ext) = entry.extension() {
            let output = match ext.to_str() {
                Some("cu") => Command::new("nvcc")
                    .args([
                        "-O3",
                        "--cubin",
                        "-lineinfo",
                        "-o",
                        &format!("{}.cubin", entry.to_string_lossy()),
                        &entry.to_string_lossy().to_string(),
                    ])
                    .output()?,
                Some("spirv") | Some("ptx") | Some("cubin") => continue,
                _ => Command::new("glslc")
                    .args([
                        &entry.to_string_lossy().to_string(),
                        "--target-env=vulkan1.3",
                        "-g",
                        "-o",
                        &format!("{}.spirv", entry.to_string_lossy()),
                        "-O",
                    ])
                    .output()?,
            };

            eprintln!("{}", String::from_utf8(output.stdout)?);
            eprintln!("{}", String::from_utf8(output.stderr)?);
            output.status.exit_ok()?;
        }
    }

    Ok(())
}

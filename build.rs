use std::process::Command;

use anyhow::bail;
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
                        "--ptx",
                        "-lineinfo",
                        "-o",
                        &format!("{}.ptx", entry.to_string_lossy()),
                        entry.to_str().unwrap(),
                    ])
                    .output()?,
                Some("hlsl") => Command::new("dxc")
                    .args([
                        "-T",
                        "cs_6_5",
                        "-spirv",
                        "-fspv-target-env=vulkan1.3",
                        "-Zi",
                        "-Fo",
                        &format!("{}.spirv", entry.to_string_lossy()),
                        entry.to_str().unwrap(),
                    ])
                    .output()?,
                Some("spirv") | Some("ptx") | Some("cubin") => continue,
                _ => Command::new("glslc")
                    .args([
                        entry.to_str().unwrap(),
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
            if !output.status.success() {
                bail!("Failed to run shader compiler");
            }
        }
    }

    Ok(())
}

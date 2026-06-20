//! Embed Kernel Trace Report HTML shell for bundled `zagens-runtime`.

use std::path::Path;

fn main() {
    let dest_dir = Path::new("assets/trace-report");
    let dest = dest_dir.join("report.html");
    let src = Path::new("../../tools/trace-report/dist/report.html");

    let _ = std::fs::create_dir_all(dest_dir);

    if src.is_file() {
        std::fs::copy(src, &dest).expect("copy trace report template into runtime-server assets");
        println!("cargo:rerun-if-changed={}", src.display());
    } else if !dest.is_file() {
        std::fs::write(
            &dest,
            "<!DOCTYPE html><html><head><title>Kernel Trace Report</title></head>\
             <body><script type=\"application/json\" id=\"trace-bundle\">__ZAGENS_TRACE_BUNDLE__</script></body></html>",
        )
        .expect("write stub trace report template");
    }

    println!("cargo:rerun-if-changed=build.rs");
}

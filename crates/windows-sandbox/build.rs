fn main() {
    #[cfg(windows)]
    {
        // WFP (Phase 2): link fwpuclnt when wfp module lands.
        println!("cargo:rustc-check-cfg=cfg(windows_sandbox_wfp)");
    }
}

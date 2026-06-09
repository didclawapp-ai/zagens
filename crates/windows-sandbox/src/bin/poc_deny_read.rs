fn main() -> anyhow::Result<()> {
    let result = zagens_windows_sandbox::run_unelevated_deny_read_poc()?;
    let path = zagens_windows_sandbox::write_poc_result(&result)?;
    println!("Gate G0 PoC result: {}", result.result);
    if let Some(notes) = &result.notes {
        println!("Notes: {notes}");
    }
    println!("Written to {}", path.display());
    if result.result != "pass" {
        std::process::exit(2);
    }
    Ok(())
}

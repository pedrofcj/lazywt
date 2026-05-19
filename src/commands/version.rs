use anyhow::Result;

pub fn run() -> Result<()> {
    println!("lazywt {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

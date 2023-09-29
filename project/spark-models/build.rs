fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(&["api.proto"], &["./protos"])?;

    Ok(())
}

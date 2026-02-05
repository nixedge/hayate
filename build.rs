// Build script to compile UTxORPC proto files

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile UTxORPC proto files
    // For now, we'll define our own simplified proto
    // Later, can use official UTxORPC spec
    
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/utxorpc/query.proto",
                "proto/utxorpc/watch.proto",
                "proto/utxorpc/submit.proto",
            ],
            &["proto"],
        )?;
    
    Ok(())
}

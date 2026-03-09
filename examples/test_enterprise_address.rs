use hayate::wallet::{Wallet, Network};
use std::env;
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // Read mnemonic from stdin (piped from gpg)
    let mut mnemonic_str = String::new();
    io::stdin().read_to_string(&mut mnemonic_str)?;
    let mnemonic_str = mnemonic_str.trim();

    if mnemonic_str.is_empty() {
        eprintln!("Error: No mnemonic provided on stdin");
        eprintln!("Usage: gpg --decrypt <file.asc> 2>/dev/null | cargo run --example test_enterprise_address [address_index]");
        std::process::exit(1);
    }

    let address_index: u32 = if args.len() > 1 {
        args[1].parse()?
    } else {
        0
    };

    // Create wallet from mnemonic
    let wallet = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0)?;

    // Derive enterprise address at the specified index
    let address = wallet.enterprise_address(address_index)?;

    println!("{}", address);

    Ok(())
}

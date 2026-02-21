// CLI handlers for wallet commands

use crate::cli::WalletCommand;
use crate::wallet::{
    WalletStorage,
    mnemonic::{generate_mnemonic, normalize_mnemonic},
    derivation::{Network, derive_payment_address},
};
use crate::gpg::Gpg;
use std::io::{self, Write};
use std::path::PathBuf;
use anyhow::{Result, Context};

/// Handle wallet commands
pub async fn handle_wallet_command(
    wallet_cmd: &WalletCommand,
    wallet_dir: PathBuf,
) -> Result<()> {
    // Create wallet storage
    let storage = WalletStorage::new(wallet_dir)?;

    match wallet_cmd {
        WalletCommand::Init { name, gpg_recipient, words, network } => {
            handle_init(&storage, name, gpg_recipient.as_deref(), *words, network)?;
        }

        WalletCommand::Add { name, mnemonic, gpg_recipient, network } => {
            handle_add(&storage, name, mnemonic.as_deref(), gpg_recipient.as_deref(), network)?;
        }

        WalletCommand::List => {
            handle_list(&storage)?;
        }

        WalletCommand::Show { name, count } => {
            handle_show(&storage, name, *count)?;
        }

        WalletCommand::Export { name } => {
            handle_export(&storage, name)?;
        }

        WalletCommand::Delete { name, yes } => {
            handle_delete(&storage, name, *yes)?;
        }

        WalletCommand::Stats { wallet: _ } => {
            println!("Stats command not yet implemented");
        }

        WalletCommand::Utxos { wallet: _ } => {
            println!("UTxOs command not yet implemented");
        }

        WalletCommand::Txs { wallet: _ } => {
            println!("Transactions command not yet implemented");
        }
    }

    Ok(())
}

fn handle_init(
    storage: &WalletStorage,
    name: &str,
    gpg_recipient: Option<&str>,
    words: usize,
    network_str: &str,
) -> Result<()> {
    // Check if GPG is required
    if let Some(recipient) = gpg_recipient {
        if !Gpg::is_available() {
            anyhow::bail!("GPG is not available. Please install gnupg or remove --gpg-recipient flag.");
        }
        println!("🔐 GPG encryption enabled with recipient: {}", recipient);
    } else {
        println!("⚠️  WARNING: Mnemonic will be stored UNENCRYPTED!");
        println!("   For production use, specify --gpg-recipient <email-or-key-id>");
        print!("   Continue without encryption? [y/N]: ");
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Parse network
    let network = parse_network(network_str)?;

    // Generate mnemonic
    println!("🎲 Generating {}-word mnemonic...", words);
    let mnemonic = generate_mnemonic(words)
        .context("Failed to generate mnemonic")?;

    println!("\n⚠️  IMPORTANT: Write down your recovery phrase and store it safely!");
    println!("════════════════════════════════════════════════════════════════");
    println!("{}", mnemonic);
    println!("════════════════════════════════════════════════════════════════");
    println!("\nPress Enter after you have securely saved your recovery phrase...");

    let mut _confirmation = String::new();
    io::stdin().read_line(&mut _confirmation)?;

    // Create wallet
    let metadata = storage.create_wallet(name, &mnemonic, network, gpg_recipient)
        .context("Failed to create wallet")?;

    println!("✅ Wallet '{}' created successfully!", metadata.name);
    println!("   Network: {}", metadata.network);
    println!("   Encrypted: {}", metadata.encrypted);
    println!("   Created: {}", format_timestamp(metadata.created_at));

    Ok(())
}

fn handle_add(
    storage: &WalletStorage,
    name: &str,
    mnemonic: Option<&str>,
    gpg_recipient: Option<&str>,
    network_str: &str,
) -> Result<()> {
    // Check if GPG is required
    if let Some(recipient) = gpg_recipient {
        if !Gpg::is_available() {
            anyhow::bail!("GPG is not available. Please install gnupg or remove --gpg-recipient flag.");
        }
        println!("🔐 GPG encryption enabled with recipient: {}", recipient);
    }

    // Parse network
    let network = parse_network(network_str)?;

    // Get mnemonic from user
    let mnemonic_phrase = if let Some(m) = mnemonic {
        m.to_string()
    } else {
        println!("Enter your recovery phrase:");
        print!("> ");
        io::stdout().flush()?;

        let mut phrase = String::new();
        io::stdin().read_line(&mut phrase)?;
        phrase.trim().to_string()
    };

    // Normalize and validate
    let normalized = normalize_mnemonic(&mnemonic_phrase);

    // Create wallet
    let metadata = storage.create_wallet(name, &normalized, network, gpg_recipient)
        .context("Failed to add wallet")?;

    println!("✅ Wallet '{}' added successfully!", metadata.name);
    println!("   Network: {}", metadata.network);
    println!("   Encrypted: {}", metadata.encrypted);
    println!("   Created: {}", format_timestamp(metadata.created_at));

    Ok(())
}

fn handle_list(storage: &WalletStorage) -> Result<()> {
    let wallets = storage.list_wallets()?;

    if wallets.is_empty() {
        println!("No wallets found.");
        println!("\nCreate a new wallet with: hayate wallet init <name>");
        return Ok(());
    }

    println!("Wallets:");
    println!("═══════════════════════════════════════════════════════════════");

    for wallet in wallets {
        let encrypted_status = if wallet.encrypted { "🔐 encrypted" } else { "⚠️  plaintext" };
        println!("  {} ({}) - {}", wallet.name, wallet.network, encrypted_status);
        println!("    Created: {}", format_timestamp(wallet.created_at));
    }

    Ok(())
}

fn handle_show(storage: &WalletStorage, name: &str, count: u32) -> Result<()> {
    let metadata = storage.load_metadata(name)
        .context("Failed to load wallet")?;

    println!("Wallet: {}", metadata.name);
    println!("═══════════════════════════════════════════════════════════════");
    println!("Network:   {}", metadata.network);
    println!("Encrypted: {}", metadata.encrypted);
    println!("Created:   {}", format_timestamp(metadata.created_at));

    // Parse network
    let network = parse_network(&metadata.network)?;

    println!("\nDeriving addresses...");

    // Derive account
    let account = storage.derive_account(name, 0)
        .context("Failed to derive account")?;

    // Show addresses
    println!("\nReceiving addresses (first {} addresses):", count);
    println!("───────────────────────────────────────────────────────────────");

    for i in 0..count {
        let address = derive_payment_address(
            &account.payment_key,
            i,
            &account.stake_key,
            network,
        ).context("Failed to derive address")?;

        println!("  {}: {}", i, address);
    }

    Ok(())
}

fn handle_export(storage: &WalletStorage, name: &str) -> Result<()> {
    println!("⚠️  WARNING: This will display your recovery phrase in plaintext!");
    print!("   Continue? [y/N]: ");
    io::stdout().flush()?;

    let mut response = String::new();
    io::stdin().read_line(&mut response)?;

    if !response.trim().eq_ignore_ascii_case("y") {
        println!("Cancelled.");
        return Ok(());
    }

    let mnemonic = storage.export_mnemonic(name)
        .context("Failed to export mnemonic")?;

    println!("\n════════════════════════════════════════════════════════════════");
    println!("{}", mnemonic);
    println!("════════════════════════════════════════════════════════════════");

    Ok(())
}

fn handle_delete(storage: &WalletStorage, name: &str, yes: bool) -> Result<()> {
    if !yes {
        println!("⚠️  WARNING: This will permanently delete wallet '{}'!", name);
        println!("   Make sure you have backed up your recovery phrase!");
        print!("   Type 'YES' to confirm: ");
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if response.trim() != "YES" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    storage.delete_wallet(name)
        .context("Failed to delete wallet")?;

    println!("✅ Wallet '{}' deleted.", name);

    Ok(())
}

fn parse_network(network_str: &str) -> Result<Network> {
    match network_str.to_lowercase().as_str() {
        "mainnet" => Ok(Network::Mainnet),
        "testnet" | "preprod" | "preview" | "sanchonet" => Ok(Network::Testnet),
        _ => anyhow::bail!("Invalid network: {}", network_str),
    }
}

fn format_timestamp(timestamp: u64) -> String {
    use std::time::{UNIX_EPOCH, Duration};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);

    // Simple formatting - just show the timestamp for now
    // In production, you might want to use chrono for better formatting
    format!("{:?}", datetime)
}

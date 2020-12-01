// Magical Bitcoin Library
// Written in 2020 by
//     Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020 Magical Bitcoin
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Command line interface
//!
//! This module provides a [structopt](https://docs.rs/crate/structopt) `struct` and `enum` that
//! parse global wallet options and wallet subcommand options needed for a wallet command line
//! interface.
//!
//! See the `repl.rs` example for how to use this module to create a simple command line REPL
//! wallet application.
//!
//! See [`WalletOpt`] for global wallet options and [`WalletSubCommand`] for supported sub-commands.
//!
//! # Example
//!
//! ```
//! use bdk::bitcoin::Network;
//! use bdk::blockchain::esplora::EsploraBlockchainConfig;
//! use bdk::blockchain::{AnyBlockchain, ConfigurableBlockchain};
//! use bdk::blockchain::{AnyBlockchainConfig, ElectrumBlockchainConfig};
//! use bdk::cli;
//! use bdk::cli::{WalletOpt, WalletSubCommand};
//! use bdk::database::MemoryDatabase;
//! use bdk::Wallet;
//! use bitcoin::hashes::core::str::FromStr;
//! use std::sync::Arc;
//! use structopt::StructOpt;
//!
//! // to get args from cli use:
//! // let cli_opt = WalletOpt::from_args();
//!
//! let cli_args = vec!["repl", "--network", "testnet", "--descriptor",
//!                     "wpkh(tpubEBr4i6yk5nf5DAaJpsi9N2pPYBeJ7fZ5Z9rmN4977iYLCGco1VyjB9tvvuvYtfZzjD5A8igzgw3HeWeeKFmanHYqksqZXYXGsw5zjnj7KM9/*)",
//!                     "sync", "--max_addresses", "50"];
//! let cli_opt = WalletOpt::from_iter(&cli_args);
//!
//! let network = Network::from_str(cli_opt.network.as_str()).unwrap_or(Network::Testnet);
//!
//! let descriptor = cli_opt.descriptor.as_str();
//! let change_descriptor = cli_opt.change_descriptor.as_deref();
//!
//! let database = MemoryDatabase::new();
//!
//! let config = match cli_opt.esplora {
//!         Some(base_url) => AnyBlockchainConfig::Esplora(EsploraBlockchainConfig {
//!         base_url: base_url.to_string(),
//!         concurrency: Some(cli_opt.esplora_concurrency),
//!     }),
//!         None => AnyBlockchainConfig::Electrum(ElectrumBlockchainConfig {
//!         url: cli_opt.electrum,
//!         socks5: cli_opt.proxy,
//!     }),
//! };
//!
//! let wallet = Wallet::new(
//!     descriptor,
//!     change_descriptor,
//!     network,
//!     database,
//!     AnyBlockchain::from_config(&config).unwrap(),
//! ).unwrap();
//!
//! let wallet = Arc::new(wallet);
//!
//! let result = cli::handle_wallet_subcommand(&wallet, cli_opt.subcommand).unwrap();
//! println!("{}", serde_json::to_string_pretty(&result).unwrap());
//! ```

use std::collections::BTreeMap;
use std::str::FromStr;

use structopt::StructOpt;

#[allow(unused_imports)]
use log::{debug, error, info, trace, LevelFilter};

use bitcoin::consensus::encode::{deserialize, serialize, serialize_hex};
use bitcoin::hashes::hex::FromHex;
use bitcoin::util::psbt::PartiallySignedTransaction;
use bitcoin::{Address, OutPoint, Script, Txid};

use crate::blockchain::log_progress;
use crate::error::Error;
use crate::types::ScriptType;
use crate::{FeeRate, TxBuilder, Wallet};

/// Wallet global options and sub-command
///
/// A [structopt](https://docs.rs/crate/structopt) `struct` that parses wallet global options and
/// sub-command from the command line or from a `String` vector. See [`WalletSubCommand`] for details
/// on parsing sub-commands.
///
/// # Example
///
/// ```
/// # use bdk::cli::{WalletOpt, WalletSubCommand};
/// # use structopt::StructOpt;
///
/// // to get WalletOpt from OS command line args use:
/// // let sync_wallet_opt = WalletOpt::from_args();
///
/// let cli_args = vec!["repl", "--network", "testnet", "--descriptor",
///                     "wpkh(tpubEBr4i6yk5nf5DAaJpsi9N2pPYBeJ7fZ5Z9rmN4977iYLCGco1VyjB9tvvuvYtfZzjD5A8igzgw3HeWeeKFmanHYqksqZXYXGsw5zjnj7KM9/*)",
///                     "sync", "--max_addresses", "50"];
/// let sync_wallet_opt = WalletOpt::from_iter(&cli_args);
///
/// assert_eq!(sync_wallet_opt.network, "testnet");
/// assert_eq!(sync_wallet_opt.wallet, "main");
/// assert_eq!(sync_wallet_opt.proxy, None);
/// assert_eq!(sync_wallet_opt.descriptor, "wpkh(tpubEBr4i6yk5nf5DAaJpsi9N2pPYBeJ7fZ5Z9rmN4977iYLCGco1VyjB9tvvuvYtfZzjD5A8igzgw3HeWeeKFmanHYqksqZXYXGsw5zjnj7KM9/*)");
/// assert_eq!(sync_wallet_opt.change_descriptor, None);
/// assert_eq!(sync_wallet_opt.verbosity, None);
/// assert!(matches!(
///         sync_wallet_opt.subcommand,
///         WalletSubCommand::Sync {
///             max_addresses: Some(50)
///         }
///     ));
/// ```
#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "BDK Wallet",
version = option_env ! ("CARGO_PKG_VERSION").unwrap_or("unknown"),
author = option_env ! ("CARGO_PKG_AUTHORS").unwrap_or(""),
about = "A modern, lightweight, descriptor-based wallet",
)]
pub struct WalletOpt {
    /// Sets the network
    #[structopt(
        name = "NETWORK",
        short = "n",
        long = "network",
        default_value = "testnet"
    )]
    pub network: String,
    /// Selects the wallet to use
    #[structopt(
        name = "WALLET_NAME",
        short = "w",
        long = "wallet",
        default_value = "main"
    )]
    pub wallet: String,
    #[cfg(feature = "electrum")]
    /// Sets the SOCKS5 proxy for the Electrum client
    #[structopt(name = "PROXY_SERVER:PORT", short = "p", long = "proxy")]
    pub proxy: Option<String>,
    /// Sets the descriptor to use for the external addresses
    #[structopt(name = "DESCRIPTOR", short = "d", long = "descriptor", required = true)]
    pub descriptor: String,
    /// Sets the descriptor to use for internal addresses
    #[structopt(name = "CHANGE_DESCRIPTOR", short = "c", long = "change_descriptor")]
    pub change_descriptor: Option<String>,
    /// Sets the level of verbosity
    #[structopt(short = "v", multiple = true)]
    pub verbosity: Option<Vec<bool>>,
    #[cfg(feature = "esplora")]
    /// Use the esplora server if given as parameter
    #[structopt(name = "ESPLORA_URL", short = "e", long = "esplora")]
    pub esplora: Option<String>,
    #[cfg(feature = "esplora")]
    /// Concurrency of requests made to the esplora server
    #[structopt(
        name = "ESPLORA_CONCURRENCY",
        long = "esplora_concurrency",
        default_value = "4"
    )]
    pub esplora_concurrency: u8,
    #[cfg(feature = "electrum")]
    /// Sets the Electrum server to use
    #[structopt(
        name = "SERVER:PORT",
        short = "s",
        long = "server",
        default_value = "ssl://electrum.blockstream.info:60002"
    )]
    pub electrum: String,
    /// Wallet sub-command
    #[structopt(subcommand)]
    pub subcommand: WalletSubCommand,
}

/// Wallet sub-command
///
/// A [structopt](https://docs.rs/crate/structopt) enum that parses wallet sub-command arguments from
/// the command line or from a `String` vector, such as in the [`repl`](https://github.com/bitcoindevkit/bdk/blob/master/examples/repl.rs)
/// example app.
///
/// Additional "external" sub-commands can be captured via the [`WalletSubCommand::Other`] enum and passed to a
/// custom `structopt` or another parser. See [structopt "External subcommands"](https://docs.rs/structopt/0.3.21/structopt/index.html#external-subcommands)
/// for more information.
///
/// # Example
///
/// ```
/// # use bdk::cli::WalletSubCommand;
/// # use structopt::StructOpt;
///
/// let sync_sub_command = WalletSubCommand::from_iter(&["repl", "sync", "--max_addresses", "50"]);
/// assert!(matches!(
///         sync_sub_command,
///         WalletSubCommand::Sync {
///             max_addresses: Some(50)
///         }
///     ));
///
/// let other_sub_command = WalletSubCommand::from_iter(&["repl", "custom", "--param1", "20"]);
/// let external_args: Vec<String> = vec!["custom".into(), "--param1".into(), "20".into()];
/// assert!(matches!(
///         other_sub_command,
///         WalletSubCommand::Other(v) if v == external_args
///     ));
/// ```
#[derive(Debug, StructOpt, Clone)]
#[structopt(rename_all = "snake")]
pub enum WalletSubCommand {
    /// Generates a new external address
    GetNewAddress,
    /// Syncs with the chosen Electrum server
    Sync {
        /// max addresses to consider
        #[structopt(short = "v", long = "max_addresses")]
        max_addresses: Option<u32>,
    },
    /// Lists the available spendable UTXOs
    ListUnspent,
    /// Lists all the incoming and outgoing transactions of the wallet
    ListTransactions,
    /// Returns the current wallet balance
    GetBalance,
    /// Creates a new unsigned transaction
    CreateTx {
        /// Adds a recipient to the transaction
        #[structopt(name = "ADDRESS:SAT", long = "to", required = true, parse(try_from_str = parse_recipient))]
        recipients: Vec<(Script, u64)>,
        /// Sends all the funds (or all the selected utxos). Requires only one recipients of value 0
        #[structopt(short = "all", long = "send_all")]
        send_all: Option<bool>,
        /// Enables Replace-By-Fee (BIP125)
        #[structopt(short = "rbf", long = "enable_rbf")]
        enable_rbf: Option<bool>,
        /// Selects which utxos *must* be spent
        #[structopt(name = "MUST_SPEND_TXID:VOUT", long = "utxos", parse(try_from_str = parse_outpoint))]
        utxos: Option<Vec<OutPoint>>,
        /// Marks a utxo as unspendable
        #[structopt(name = "CANT_SPEND_TXID:VOUT", long = "unspendable", parse(try_from_str = parse_outpoint))]
        unspendable: Option<Vec<OutPoint>>,
        /// Fee rate to use in sat/vbyte
        #[structopt(name = "SATS_VBYTE", short = "fee", long = "fee_rate")]
        fee_rate: Option<f32>,
        /// Selects which policy should be used to satisfy the external descriptor
        #[structopt(name = "EXT_POLICY", long = "external_policy")]
        external_policy: Option<String>,
        /// Selects which policy should be used to satisfy the internal descriptor
        #[structopt(name = "INT_POLICY", long = "internal_policy")]
        internal_policy: Option<String>,
    },
    /// Bumps the fees of an RBF transaction
    BumpFee {
        /// TXID of the transaction to update
        #[structopt(name = "TXID", short = "txid", long = "txid")]
        txid: String,
        /// Allows the wallet to reduce the amount of the only output in order to increase fees. This is generally the expected behavior for transactions originally created with `send_all`
        #[structopt(short = "all", long = "send_all")]
        send_all: Option<bool>,
        /// Selects which utxos *must* be added to the tx. Unconfirmed utxos cannot be used
        #[structopt(name = "MUST_SPEND_TXID:VOUT", long = "utxos", parse(try_from_str = parse_outpoint))]
        utxos: Option<Vec<OutPoint>>,
        /// Marks an utxo as unspendable, in case more inputs are needed to cover the extra fees
        #[structopt(name = "CANT_SPEND_TXID:VOUT", long = "unspendable", parse(try_from_str = parse_outpoint))]
        unspendable: Option<Vec<OutPoint>>,
        /// The new targeted fee rate in sat/vbyte
        #[structopt(name = "SATS_VBYTE", short = "fee", long = "fee_rate")]
        fee_rate: f32,
    },
    /// Returns the available spending policies for the descriptor
    Policies,
    /// Returns the public version of the wallet's descriptor(s)
    PublicDescriptor,
    /// Signs and tries to finalize a PSBT
    Sign {
        /// Sets the PSBT to sign
        #[structopt(name = "BASE64_PSBT", long = "psbt")]
        psbt: String,
        /// Assume the blockchain has reached a specific height. This affects the transaction finalization, if there are timelocks in the descriptor
        #[structopt(name = "HEIGHT", long = "assume_height")]
        assume_height: Option<u32>,
    },
    /// Broadcasts a transaction to the network. Takes either a raw transaction or a PSBT to extract
    Broadcast {
        /// Sets the PSBT to sign
        #[structopt(
            name = "BASE64_PSBT",
            long = "psbt",
            required_unless = "RAWTX",
            conflicts_with = "RAWTX"
        )]
        psbt: Option<String>,
        /// Sets the raw transaction to broadcast
        #[structopt(
            name = "RAWTX",
            long = "tx",
            required_unless = "BASE64_PSBT",
            conflicts_with = "BASE64_PSBT"
        )]
        tx: Option<String>,
    },
    /// Extracts a raw transaction from a PSBT
    ExtractPsbt {
        /// Sets the PSBT to extract
        #[structopt(name = "BASE64_PSBT", long = "psbt")]
        psbt: String,
    },
    /// Finalizes a PSBT
    FinalizePsbt {
        /// Sets the PSBT to finalize
        #[structopt(name = "BASE64_PSBT", long = "psbt")]
        psbt: String,
        /// Assume the blockchain has reached a specific height
        #[structopt(name = "HEIGHT", long = "assume_height")]
        assume_height: Option<u32>,
    },
    /// Combines multiple PSBTs into one
    CombinePsbt {
        /// Add one PSBT to combine. This option can be repeated multiple times, one for each PSBT
        #[structopt(name = "BASE64_PSBT", long = "psbt", required = true)]
        psbt: Vec<String>,
    },
    /// Put any extra arguments into this Vec
    #[structopt(external_subcommand)]
    Other(Vec<String>),
}

fn parse_recipient(s: &str) -> Result<(Script, u64), String> {
    let parts: Vec<_> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("Invalid format".to_string());
    }

    let addr = Address::from_str(&parts[0]);
    if let Err(e) = addr {
        return Err(format!("{:?}", e));
    }
    let val = u64::from_str(&parts[1]);
    if let Err(e) = val {
        return Err(format!("{:?}", e));
    }

    Ok((addr.unwrap().script_pubkey(), val.unwrap()))
}

fn parse_outpoint(s: &str) -> Result<OutPoint, String> {
    OutPoint::from_str(s).map_err(|e| format!("{:?}", e))
}

/// Execute a wallet sub-command with a given [`Wallet`].
///
/// Wallet sub-commands are described in [`WalletSubCommand`]. See [`super::cli`] for example usage.
#[maybe_async]
pub fn handle_wallet_subcommand<C, D>(
    wallet: &Wallet<C, D>,
    wallet_subcommand: WalletSubCommand,
) -> Result<serde_json::Value, Error>
where
    C: crate::blockchain::Blockchain,
    D: crate::database::BatchDatabase,
{
    match wallet_subcommand {
        WalletSubCommand::GetNewAddress => Ok(json!({"address": wallet.get_new_address()?})),
        WalletSubCommand::Sync { max_addresses } => {
            maybe_await!(wallet.sync(log_progress(), max_addresses))?;
            Ok(json!({}))
        }
        WalletSubCommand::ListUnspent => Ok(serde_json::to_value(&wallet.list_unspent()?)?),
        WalletSubCommand::ListTransactions => {
            Ok(serde_json::to_value(&wallet.list_transactions(false)?)?)
        }
        WalletSubCommand::GetBalance => Ok(json!({"satoshi": wallet.get_balance()?})),
        WalletSubCommand::CreateTx {
            recipients,
            send_all,
            enable_rbf,
            utxos,
            unspendable,
            fee_rate,
            external_policy,
            internal_policy,
        } => {
            let mut tx_builder = TxBuilder::new();

            if send_all.unwrap_or(false) {
                tx_builder = tx_builder
                    .drain_wallet()
                    .set_single_recipient(recipients[0].0.clone());
            } else {
                tx_builder = tx_builder.set_recipients(recipients);
            }

            if enable_rbf.unwrap_or(false) {
                tx_builder = tx_builder.enable_rbf();
            }

            if let Some(fee_rate) = fee_rate {
                tx_builder = tx_builder.fee_rate(FeeRate::from_sat_per_vb(fee_rate));
            }

            if let Some(utxos) = utxos {
                tx_builder = tx_builder.utxos(utxos).manually_selected_only();
            }

            if let Some(unspendable) = unspendable {
                tx_builder = tx_builder.unspendable(unspendable);
            }

            let policies = vec![
                external_policy.map(|p| (p, ScriptType::External)),
                internal_policy.map(|p| (p, ScriptType::Internal)),
            ];

            for (policy, script_type) in policies.into_iter().filter_map(|x| x) {
                let policy = serde_json::from_str::<BTreeMap<String, Vec<usize>>>(&policy)
                    .map_err(|s| Error::Generic(s.to_string()))?;
                tx_builder = tx_builder.policy_path(policy, script_type);
            }

            let (psbt, details) = wallet.create_tx(tx_builder)?;
            Ok(json!({"psbt": base64::encode(&serialize(&psbt)),"details": details,}))
        }
        WalletSubCommand::BumpFee {
            txid,
            send_all,
            utxos,
            unspendable,
            fee_rate,
        } => {
            let txid = Txid::from_str(txid.as_str()).map_err(|s| Error::Generic(s.to_string()))?;

            let mut tx_builder = TxBuilder::new().fee_rate(FeeRate::from_sat_per_vb(fee_rate));

            if send_all.unwrap_or(false) {
                tx_builder = tx_builder.maintain_single_recipient();
            }

            if let Some(utxos) = utxos {
                tx_builder = tx_builder.utxos(utxos);
            }

            if let Some(unspendable) = unspendable {
                tx_builder = tx_builder.unspendable(unspendable);
            }

            let (psbt, details) = wallet.bump_fee(&txid, tx_builder)?;
            Ok(json!({"psbt": base64::encode(&serialize(&psbt)),"details": details,}))
        }
        WalletSubCommand::Policies => Ok(json!({
            "external": wallet.policies(ScriptType::External)?,
            "internal": wallet.policies(ScriptType::Internal)?,
        })),
        WalletSubCommand::PublicDescriptor => Ok(json!({
            "external": wallet.public_descriptor(ScriptType::External)?.map(|d| d.to_string()),
            "internal": wallet.public_descriptor(ScriptType::Internal)?.map(|d| d.to_string()),
        })),
        WalletSubCommand::Sign {
            psbt,
            assume_height,
        } => {
            let psbt = base64::decode(&psbt).unwrap();
            let psbt: PartiallySignedTransaction = deserialize(&psbt).unwrap();
            let (psbt, finalized) = wallet.sign(psbt, assume_height)?;
            Ok(json!({"psbt": base64::encode(&serialize(&psbt)),"is_finalized": finalized,}))
        }
        WalletSubCommand::Broadcast { psbt, tx } => {
            let tx = if psbt.is_some() {
                let psbt = base64::decode(&psbt.unwrap()).unwrap();
                let psbt: PartiallySignedTransaction = deserialize(&psbt).unwrap();
                psbt.extract_tx()
            } else if tx.is_some() {
                deserialize(&Vec::<u8>::from_hex(&tx.unwrap()).unwrap()).unwrap()
            } else {
                panic!("Missing `psbt` and `tx` option");
            };

            let txid = maybe_await!(wallet.broadcast(tx))?;
            Ok(json!({ "txid": txid }))
        }
        WalletSubCommand::ExtractPsbt { psbt } => {
            let psbt = base64::decode(&psbt).unwrap();
            let psbt: PartiallySignedTransaction = deserialize(&psbt).unwrap();
            Ok(json!({"raw_tx": serialize_hex(&psbt.extract_tx()),}))
        }
        WalletSubCommand::FinalizePsbt {
            psbt,
            assume_height,
        } => {
            let psbt = base64::decode(&psbt).unwrap();
            let psbt: PartiallySignedTransaction = deserialize(&psbt).unwrap();

            let (psbt, finalized) = wallet.finalize_psbt(psbt, assume_height)?;
            Ok(json!({ "psbt": base64::encode(&serialize(&psbt)),"is_finalized": finalized,}))
        }
        WalletSubCommand::CombinePsbt { psbt } => {
            let mut psbts = psbt
                .iter()
                .map(|s| {
                    let psbt = base64::decode(&s).unwrap();
                    let psbt: PartiallySignedTransaction = deserialize(&psbt).unwrap();
                    psbt
                })
                .collect::<Vec<_>>();

            let init_psbt = psbts.pop().unwrap();
            let final_psbt = psbts
                .into_iter()
                .try_fold::<_, _, Result<PartiallySignedTransaction, Error>>(
                    init_psbt,
                    |mut acc, x| {
                        acc.merge(x)?;
                        Ok(acc)
                    },
                )?;

            Ok(json!({ "psbt": base64::encode(&serialize(&final_psbt)) }))
        }
        WalletSubCommand::Other(_) => Ok(json!({})),
    }
}

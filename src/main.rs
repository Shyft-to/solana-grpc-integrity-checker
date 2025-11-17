use {
    backoff::{ExponentialBackoff, future::retry},
    clap::Parser,
    futures::{SinkExt, StreamExt},
    log::{error, info},
    solana_client::{
        rpc_client::RpcClient,
        rpc_config::{CommitmentConfig, RpcBlockConfig, TransactionDetails},
    },
    std::{
        collections::HashMap,
        env,
        sync::{Arc, Mutex},
        time::Duration,
    },
    tokio::time::Instant,
    yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient, Interceptor},
    yellowstone_grpc_proto::geyser::{
        CommitmentLevel, SubscribeRequest, SubscribeRequestFilterBlocks, SubscribeRequestPing,
        subscribe_update::UpdateOneof,
    },
};

type BlockFilterMap = HashMap<String, SubscribeRequestFilterBlocks>;

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(long)]
    endpoint: String,
    #[clap(long)]
    x_token: String,

    #[clap(long)]
    rpc_uri: String,

    #[clap(long, default_value = "60")] duration: u64, // seconds
}

impl Args {
    async fn connect(&self) -> anyhow::Result<GeyserGrpcClient<impl Interceptor>> {
        let client = GeyserGrpcClient::build_from_shared(self.endpoint.to_owned())?
            .x_token(Some(self.x_token.to_owned()))?
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .connect()
            .await?;
        Ok(client)
    }

    fn build_blocks_request(&self) -> SubscribeRequest {
        let mut blocks: BlockFilterMap = HashMap::new();
        blocks.insert(
            "client".to_string(),
            SubscribeRequestFilterBlocks {
                account_include: vec![],
                include_transactions: Some(true),
                include_accounts: None,
                include_entries: None,
            },
        );

        SubscribeRequest {
            blocks,
            commitment: Some(CommitmentLevel::Finalized as i32),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
struct Report {
    total_blocks: u64,
    mismatched_blocks: u64,
    total_grpc_txs: u64,
    total_rpc_txs: u64,
    details: Vec<String>,
}

async fn run_stream_for_duration(args: Args, request: SubscribeRequest, duration: Duration) {
    let report = Arc::new(Mutex::new(Report::default()));
    let start = Instant::now();

    let _: Result<(), anyhow::Error> = retry(ExponentialBackoff::default(), || {
        let args = args.clone();
        let request = request.clone();
        let report = report.clone();
        async move {
            let mut client = args.connect().await.map_err(backoff::Error::transient)?;
            let (mut tx, mut stream) = client
                .subscribe()
                .await
                .map_err(|e| backoff::Error::transient(anyhow::anyhow!(e)))?;

            tx.send(request.clone())
                .await
                .map_err(|e| backoff::Error::transient(anyhow::anyhow!(e)))?;

            while let Some(msg) = stream.next().await {
                if start.elapsed() >= duration {
                    info!("Timer finished — stopping stream...");
                    return Err(backoff::Error::permanent(anyhow::anyhow!(
                        "timer completed"
                    )));
                }

                let Ok(update) = msg else { break };

                if let Some(UpdateOneof::Block(block)) = update.update_oneof {
                    let slot = block.slot;
                    // let grpc_tx_count = block.transactions.len() as u64;
                    let grpc_tx_count = block.executed_transaction_count;

                    {
                        let mut rep = report.lock().unwrap();
                        rep.total_blocks += 1;
                        rep.total_grpc_txs += grpc_tx_count;
                    }

                    if let Err(e) =
                        compare_with_rpc(&args.rpc_uri, slot, grpc_tx_count, &report).await
                    {
                        error!("RPC comparison error: {:?}", e);
                    }
                } else if let Some(UpdateOneof::Ping(_)) = update.update_oneof {
                    let _ = tx
                        .send(SubscribeRequest {
                            ping: Some(SubscribeRequestPing { id: 1 }),
                            ..Default::default()
                        })
                        .await;
                }
            }

            Err(backoff::Error::transient(anyhow::anyhow!("Stream ended")))
        }
    })
    .await;

    print_final_report(&report);
}

async fn compare_with_rpc(
    rpc_url: &str,
    slot: u64,
    grpc_count: u64,
    report: &Arc<Mutex<Report>>,
) -> anyhow::Result<()> {
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::finalized());

    let block: solana_client::rpc_response::UiConfirmedBlock = client.get_block_with_config(
        slot,
        RpcBlockConfig {
            encoding: None,
            transaction_details: Some(TransactionDetails::Signatures),
            rewards: None,
            commitment: Some(CommitmentConfig::finalized()),
            max_supported_transaction_version: Some(0),
        },
    )?;

    let rpc_count = block.signatures.map_or(0, |sigs| sigs.len() as u64);

    let mut rep = report.lock().unwrap();
    rep.total_rpc_txs += rpc_count;

    if grpc_count != rpc_count {
        rep.mismatched_blocks += 1;

        rep.details.push(format!(
            "Slot {} mismatch: gRPC Tx Count={} RPC Tx Count={}",
            slot, grpc_count, rpc_count
        ));

        info!(
            "MISMATCH slot {} → gRPC Tx Count={} RPC Tx Count={}",
            slot, grpc_count, rpc_count
        );
    } else {
        info!("MATCH slot {} → gRPC Tx Count={} RPC Tx Count={}", slot, grpc_count, rpc_count);
    }

    Ok(())
}

fn print_final_report(report: &Arc<Mutex<Report>>) {
    let rep = report.lock().unwrap();

    println!("\n============== FINAL REPORT ==============");
    println!("Total Blocks Received: {}", rep.total_blocks);
    println!("Total gRPC Tx Count: {}", rep.total_grpc_txs);
    println!("Total RPC Tx Count: {}", rep.total_rpc_txs);
    println!("Mismatched Blocks: {}", rep.mismatched_blocks);

    if !rep.details.is_empty() {
        println!("\n--- MISMATCH DETAILS ---");
        for d in &rep.details {
            println!("{}", d);
        }
    }
    println!("===========================================");
}

fn main() -> anyhow::Result<()> {
    unsafe {
        env::set_var(
            env_logger::DEFAULT_FILTER_ENV,
            env::var_os(env_logger::DEFAULT_FILTER_ENV).unwrap_or_else(|| "info".into()),
        );
    }
    env_logger::init();

    let args = Args::parse();

    let blocks_request = args.build_blocks_request();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let stream_handle = tokio::spawn(run_stream_for_duration(
            args.clone(),
            blocks_request,
            Duration::from_secs(args.duration),
        ));

        stream_handle.await?;

        Ok(())
    })
}

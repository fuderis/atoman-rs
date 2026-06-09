#![cfg(feature = "logger")]
use atoman::{Instrument, Level, Logger, Span, error, info, log, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // init logger:
    Logger::init(".logs", 1000).await?;
    Logger::set_level(Level::INFO).await;

    // handle requests:
    tokio::join!(
        handle_transfer_request(987129485, 1337, "ACCOUNT-EUR-4412", "CARD-VISA-9981"),
        handle_transfer_request(100000002, 7777, "ACCOUNT-USD-5555", "CARD-MASTERCARD-1111"),
    );

    // before exit, force write the logger buffer to disk:
    Logger::flush().await;
    Ok(())
}

#[log(skip_all, fields(sid = %session_id, uid = %user_id))]
async fn handle_transfer_request(
    session_id: u64,
    user_id: u64,
    source_account: &str,
    destination_account: &str,
) {
    info!(
        source = %source_account,
        destination = %destination_account,
        "Transfer request received"
    );

    if validate_source_balance(source_account, 500).await {
        let current_context = Span::current();
        let destination = destination_account.to_string();

        let (sid, uid) = (session_id.to_string(), user_id.to_string());
        let task = tokio::spawn(
            async move {
                if let Err(e) = dispatch_to_gateway(&destination, user_id).await {
                    error!(error = %e, "Transfer pipeline aborted");

                    match Logger::trace(&[sid, uid]).await {
                        Ok(isolated_logs) => {
                            tokio::fs::create_dir_all(".logs/errors").await.ok();
                            tokio::fs::write(Logger::gen_path(".logs/errors"), &isolated_logs)
                                .await
                                .unwrap();

                            #[cfg(debug_assertions)]
                            {
                                println!("\n\x1b[35m[ERROR TRACED] for UID {}:", user_id);
                                println!("{}\x1b[0m", isolated_logs);
                            }
                        }

                        Err(err) => {
                            eprintln!("Orchestrator failed to trace logs: {err}");
                        }
                    }
                }
            }
            .instrument(current_context),
        );

        let _ = task.await;
    } else {
        warn!(source = %source_account, "Transfer rejected: validation failed");
    }

    info!("Transfer request lifecycle ended");
}

#[log(skip_all, fields(account = %account_number))]
async fn validate_source_balance(account_number: &str, transaction_amount: u32) -> bool {
    let is_balance_valid = transaction_amount < 1000;

    if is_balance_valid {
        info!(amount = transaction_amount, "Balance check passed");
    } else {
        error!(
            amount = transaction_amount,
            "Balance check failed: insufficient funds"
        );
    }

    is_balance_valid
}

#[log(skip_all, fields(target = %destination_account))]
async fn dispatch_to_gateway(destination_account: &str, user_id: u64) -> Result<(), &'static str> {
    info!("Routing transaction to external clearing network");
    execute_external_settlement(user_id).await
}

#[log(skip_all)]
async fn execute_external_settlement(user_id: u64) -> Result<(), &'static str> {
    let is_gateway_responsive = user_id == 7777;

    if is_gateway_responsive {
        info!("Settlement cleared by remote gateway");
        Ok(())
    } else {
        Err("Gateway timeout: remote clearing house did not respond")
    }
}

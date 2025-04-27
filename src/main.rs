use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;

use client::Client;

pub mod client;
pub mod graph;

#[derive(Parser)]
struct Args {
    #[clap(short, long, default_value = "127.0.0.1:0")]
    local_addr: SocketAddr,

    #[clap(short, long, default_value = "127.0.0.1:5050")]
    remote_addr: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let client = Client::bind(args.local_addr, args.remote_addr).await?;

    client
        .exec(
            r#"
                    mix[1] = function()
                        return sine_oscillator(440) * 0.1
                    end

                    play()
                    sleep(1)
                    stop()

                "#
            .trim(),
        )
        .await?;

    Ok(())
}

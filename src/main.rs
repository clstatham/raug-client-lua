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
                    local sine = sine_oscillator(220)
                    local mix = sine * 0.1
                    dac(mix)
                    dac(mix)

                    play()
                    sleep(1)
                    sine:replace(bl_saw_oscillator(110))
                    sleep(1)
                    stop()

                "#
            .trim(),
        )
        .await?;

    Ok(())
}

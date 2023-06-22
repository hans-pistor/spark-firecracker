use clap::Parser;
use spark_lib::api::{vm_actions_client::VmActionsClient, PingRequest};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "localhost")]
    address: String,
    #[arg(long, default_value = "3000")]
    port: String,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut client =
        VmActionsClient::connect(format!("http://{}:{}", args.address, args.port)).await?;

    let request = tonic::Request::new(PingRequest {});
    let response = client.ping(request).await?;

    println!("Response = {response:?}");

    Ok(())
}

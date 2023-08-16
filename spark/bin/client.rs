use clap::{Parser, ValueEnum};
use spark_lib::api::{
    vm_actions_client::VmActionsClient, GetDmesgRequest, PingRequest, ShutdownRequest,
};

#[derive(Clone, Debug, ValueEnum)]
enum CommandKind {
    Ping,
    Shutdown,
    GetDmesg,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long)]
    command: CommandKind,
    #[arg(long, default_value = "localhost")]
    address: String,
    #[arg(long, default_value = "3000")]
    port: String,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    println!(
        "Connecting to {}:{} and executing {:?}",
        args.address, args.port, args.command
    );
    let mut client =
        VmActionsClient::connect(format!("http://{}:{}", args.address, args.port)).await?;

    match args.command {
        CommandKind::Ping => {
            let request = tonic::Request::new(PingRequest {});
            let response = client.ping(request).await?;
            println!("Response = {response:?}");
        }
        CommandKind::Shutdown => {
            let request = tonic::Request::new(ShutdownRequest {});
            let response = client.shutdown(request).await?;
            println!("Response = {response:?}");
        }
        CommandKind::GetDmesg => {
            let request = tonic::Request::new(GetDmesgRequest {});
            let response = client.get_dmesg(request).await?;
            let text = response.into_inner().text;
            println!("{text}");
        }
    };

    Ok(())
}

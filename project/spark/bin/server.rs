use clap::Parser;
use spark_models::api::vm_actions_server::VmActionsServer;
use spark_models::VmService;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    address: String,
    #[arg(long, default_value = "3000")]
    port: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let socket_addr = &format!("{}:{}", args.address, args.port).parse()?;

    let grpc =
        tonic::transport::Server::builder().add_service(VmActionsServer::new(VmService::default()));

    grpc.serve(*socket_addr).await?;

    Ok(())
}

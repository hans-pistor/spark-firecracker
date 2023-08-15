use clap::Parser;
use spark_lib::{
    net::IpTablesGuard,
    vm::{
        models::{VmBootSource, VmDrive},
        VirtualMachine,
    },
};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "ens33")]
    host_network_interface: String,

    #[arg(long)]
    firecracker_path: String,

    #[arg(long)]
    kernel_image_path: String,

    #[arg(long, default_value = "console=ttyS0 reboot=k panic=1 pci=off")]
    boot_args: String,

    #[arg(long)]
    root_fs_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Args::parse();
    let _guard = IpTablesGuard::new()?;

    let _machine = VirtualMachine::new(&config.firecracker_path, 0)
        .await?
        .with_logger()
        .await?
        .setup_boot_source(VmBootSource {
            kernel_image_path: config.kernel_image_path.clone(),
            boot_args: config.boot_args.clone(),
        })
        .await?
        .with_drive(VmDrive {
            drive_id: "rootfs".into(),
            path_on_host: config.root_fs_path.clone(),
            is_root_device: true,
            is_read_only: false,
        })
        .await?
        .with_drive(VmDrive {
            drive_id: "block".into(),
            path_on_host: "/tmp/block.ext4".into(),
            is_root_device: false,
            is_read_only: true,
        })
        .await?
        .add_network_interface(&config.host_network_interface)
        .await?
        .start()
        .await?;

    tokio::time::sleep(std::time::Duration::from_secs(90)).await;

    Ok(())
}

use clap::Parser;
use spark_lib::{
    net::{IpTablesGuard},
    vm::{
        models::{VmBootSource, VmDrive},
        VirtualMachine, VmStarted,
    },
};

pub const BRIDGE_IP: &str = "172.16.0.1";

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "ens33")]
    host_network_interface: String,

    #[arg(long)]
    firecracker_path: String,

    #[arg(long)]
    kernel_image_path: String,

    #[arg(
        long,
        default_value = "console=ttyS0 reboot=k panic=1 pci=off nomodules ipv6.disable=1 8250.nr_uarts=0  tsc=reliable quiet i8042.nokbd i8042.noaux"
    )]
    boot_args: String,

    #[arg(long)]
    root_fs_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Args::parse();
    let _guard = IpTablesGuard::new(&config.host_network_interface)?;

    let _vm1 = spawn_vm(&config, 0).await?;

    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    Ok(())
}

async fn spawn_vm(config: &Args, vm_id: usize) -> anyhow::Result<VirtualMachine<VmStarted>> {
    assert!(vm_id + 2 < 256);
    let boot_source = VmBootSource {
        kernel_image_path: config.kernel_image_path.clone(),
        boot_args: config.boot_args.clone(),
    };
    let rootfs = VmDrive {
        drive_id: "rootfs".into(),
        path_on_host: config.root_fs_path.clone(),
        is_root_device: true,
        is_read_only: false,
    };

    VirtualMachine::new(&config.firecracker_path, vm_id, boot_source, rootfs)
        .await?
        .with_logger()
        .await?
        .add_network_interface(&config.host_network_interface, "172.16.0.2", BRIDGE_IP)
        .await?
        .start()
        .await
}

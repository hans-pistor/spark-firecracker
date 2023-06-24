use clap::Parser;
use spark_lib::{
    net::{IpTablesGuard, VmNetwork},
    vm::{
        models::{VmBootSource, VmDrive, VmNetworkInterface},
        VirtualMachine,
    },
};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "ens33")]
    host_network_interface: String,

    #[arg(
        long,
        default_value = "/home/hpistor/firecracker/build/cargo_target/x86_64-unknown-linux-musl/debug/firecracker"
    )]
    firecracker_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _guard = IpTablesGuard::new()?;

    let machine = VirtualMachine::new(&args.firecracker_path, 0)
        .await?
        .with_logger()
        .await?
        .setup_boot_source(VmBootSource {
            kernel_image_path: "/tmp/vmlinux.bin".into(),
            boot_args: "console=ttyS0 reboot=k panic=1 pci=off".into(),
        })
        .await?
        .setup_root_fs(VmDrive {
            drive_id: "rootfs".into(),
            path_on_host: "/tmp/rootfs.ext4".into(),
            is_root_device: true,
            is_read_only: false,
        })
        .await?
        .add_network_interface(&args.host_network_interface)
        .await?
        .start()
        .await?;

    tokio::time::sleep(std::time::Duration::from_secs(90)).await;

    Ok(())
}
